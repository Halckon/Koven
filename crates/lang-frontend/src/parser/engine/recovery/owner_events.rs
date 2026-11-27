use super::{TerminalOwnerEvent, TerminalOwnerKind};
use crate::lexer::{Lexeme, LexemeKind, TokenKind};
use crate::parser::ParserInternalError;

/// 词法 owner 栈验证器的存储抽象：
/// - 生产路径使用 [`VecOwnerBuffers`] 按实际 owner 深度与事件数按需增长（无字符串/无错误时零堆分配）；
/// - Kani 符号检验路径使用固定容量栈数组避免 CBMC 堆分配路径爆炸。
pub(super) trait OwnerEventBuffers {
    fn push_active(&mut self, kind: TerminalOwnerKind, opener: usize);
    fn last_active(&self) -> Option<(TerminalOwnerKind, usize)>;
    fn pop_active(&mut self) -> Option<(TerminalOwnerKind, usize)>;
    fn is_active_empty(&self) -> bool;
    fn push_verified(&mut self, event: TerminalOwnerEvent);
}

struct VecOwnerBuffers {
    active_owners: Vec<(TerminalOwnerKind, usize)>,
    verified_events: Vec<TerminalOwnerEvent>,
}

impl VecOwnerBuffers {
    fn new(terminal_event_hint: usize) -> Self {
        Self {
            active_owners: Vec::new(),
            verified_events: Vec::with_capacity(terminal_event_hint),
        }
    }
}

impl OwnerEventBuffers for VecOwnerBuffers {
    fn push_active(&mut self, kind: TerminalOwnerKind, opener: usize) {
        self.active_owners.push((kind, opener));
    }

    fn last_active(&self) -> Option<(TerminalOwnerKind, usize)> {
        self.active_owners.last().copied()
    }

    fn pop_active(&mut self) -> Option<(TerminalOwnerKind, usize)> {
        self.active_owners.pop()
    }

    fn is_active_empty(&self) -> bool {
        self.active_owners.is_empty()
    }

    fn push_verified(&mut self, event: TerminalOwnerEvent) {
        self.verified_events.push(event);
    }
}

/// 验证词法 owner 栈（`StringStart`/`InterpolationStart` 与对应闭合及终止事件）的 LIFO
/// 平衡性，并将隐式 EOF 级联关闭与终止性无效转义折叠为有序的 `verified_events`。
///
/// 由 `LexicalRecoveryIndex::new` 调用，按实际 owner 深度与终止事件数按需分配，
/// 无字符串与词法错误时不产生堆分配，保持与原内联循环完全一致的内存与语义行为。
pub(super) fn verify_terminal_owner_events(
    lexemes: &[Lexeme],
    terminal_owner_events: &[TerminalOwnerEvent],
    terminal_escape_spans: &[(usize, usize)],
    terminal_error_at_eof: bool,
    source_len: usize,
) -> Result<Vec<TerminalOwnerEvent>, ParserInternalError> {
    let mut buffers = VecOwnerBuffers::new(terminal_owner_events.len());
    verify_terminal_owner_events_with(
        lexemes,
        terminal_owner_events,
        terminal_escape_spans,
        terminal_error_at_eof,
        source_len,
        &mut buffers,
    )?;
    Ok(buffers.verified_events)
}

/// 词法 owner 栈验证核心状态机，通过 [`OwnerEventBuffers`] 复用于生产 `Vec` 与 Kani 固定栈缓冲。
pub(super) fn verify_terminal_owner_events_with<B: OwnerEventBuffers>(
    lexemes: &[Lexeme],
    terminal_owner_events: &[TerminalOwnerEvent],
    terminal_escape_spans: &[(usize, usize)],
    terminal_error_at_eof: bool,
    source_len: usize,
    buffers: &mut B,
) -> Result<(), ParserInternalError> {
    let mut next_terminal_event = 0usize;

    for lexeme in lexemes {
        while terminal_owner_events
            .get(next_terminal_event)
            .is_some_and(|event| event.offset <= lexeme.span().start())
        {
            let event = terminal_owner_events[next_terminal_event];
            if buffers.last_active() != Some((event.kind, event.opener)) {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            buffers.pop_active();
            buffers.push_verified(event);
            next_terminal_event += 1;
        }

        if matches!(lexeme.kind(), LexemeKind::Eof) && !buffers.is_active_empty() {
            // 终止性 char/comment 根因会按 Lexer 的级联抑制规则取代外层 string / interpolation
            // 诊断；此时没有显式 owner event，但剩余 owner 仍必须在同一 EOF 关闭。
            if !terminal_error_at_eof {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            while let Some((kind, opener)) = buffers.pop_active() {
                buffers.push_verified(TerminalOwnerEvent {
                    offset: source_len,
                    kind,
                    opener,
                });
            }
        }

        match lexeme.kind() {
            LexemeKind::Token(TokenKind::StringStart) => {
                buffers.push_active(TerminalOwnerKind::String, lexeme.span().start())
            }
            LexemeKind::Token(TokenKind::InterpolationStart) => {
                buffers.push_active(TerminalOwnerKind::Interpolation, lexeme.span().start())
            }
            LexemeKind::Token(TokenKind::StringEnd) => {
                if !matches!(buffers.pop_active(), Some((TerminalOwnerKind::String, _))) {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                if !matches!(
                    buffers.pop_active(),
                    Some((TerminalOwnerKind::Interpolation, _))
                ) {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
            }
            LexemeKind::Invalid(_)
                if terminal_escape_spans
                    .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                    .is_ok() =>
            {
                let Some((TerminalOwnerKind::String, opener)) = buffers.last_active() else {
                    return Err(ParserInternalError::InvalidLexemeStream);
                };
                let event = TerminalOwnerEvent {
                    offset: lexeme.span().end(),
                    kind: TerminalOwnerKind::String,
                    opener,
                };
                buffers.pop_active();
                buffers.push_verified(event);
            }
            _ => {}
        }
    }

    if next_terminal_event != terminal_owner_events.len() {
        return Err(ParserInternalError::InvalidLexemeStream);
    }
    Ok(())
}
