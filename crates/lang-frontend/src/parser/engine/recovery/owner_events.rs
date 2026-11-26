use super::{TerminalOwnerEvent, TerminalOwnerKind};
use crate::lexer::{Lexeme, LexemeKind, TokenKind};
use crate::parser::ParserInternalError;

/// 验证词法 owner 栈（`StringStart`/`InterpolationStart` 与对应闭合及终止事件）的 LIFO
/// 平衡性，并将隐式 EOF 级联关闭与终止性无效转义折叠为有序的 `verified_events`。
///
/// 由 `LexicalRecoveryIndex::new` 调用，保持与内联循环完全一致的拒绝与输出语义。
pub(super) fn verify_terminal_owner_events(
    lexemes: &[Lexeme],
    terminal_owner_events: &[TerminalOwnerEvent],
    terminal_escape_spans: &[(usize, usize)],
    terminal_error_at_eof: bool,
    source_len: usize,
) -> Result<Vec<TerminalOwnerEvent>, ParserInternalError> {
    let mut active_buf = vec![(TerminalOwnerKind::String, 0); lexemes.len()];
    let mut verified_buf = vec![
        TerminalOwnerEvent {
            offset: 0,
            kind: TerminalOwnerKind::String,
            opener: 0,
        };
        terminal_owner_events.len() + lexemes.len()
    ];
    let verified_len = verify_terminal_owner_events_into(
        lexemes,
        terminal_owner_events,
        terminal_escape_spans,
        terminal_error_at_eof,
        source_len,
        &mut active_buf,
        &mut verified_buf,
    )?;
    verified_buf.truncate(verified_len);
    Ok(verified_buf)
}

/// 零堆分配核心状态机：使用调用方提供的 `active_buf`（长度 `>= lexemes.len()`）与
/// `verified_buf`（长度 `>= terminal_owner_events.len() + lexemes.len()`）执行词法 owner 栈验证，
/// 返回写入 `verified_buf[..len]` 的事件数。
pub(super) fn verify_terminal_owner_events_into(
    lexemes: &[Lexeme],
    terminal_owner_events: &[TerminalOwnerEvent],
    terminal_escape_spans: &[(usize, usize)],
    terminal_error_at_eof: bool,
    source_len: usize,
    active_buf: &mut [(TerminalOwnerKind, usize)],
    verified_buf: &mut [TerminalOwnerEvent],
) -> Result<usize, ParserInternalError> {
    debug_assert!(active_buf.len() >= lexemes.len());
    debug_assert!(verified_buf.len() >= terminal_owner_events.len() + lexemes.len());

    let mut active_len = 0usize;
    let mut verified_len = 0usize;
    let mut next_terminal_event = 0usize;

    for lexeme in lexemes {
        while terminal_owner_events
            .get(next_terminal_event)
            .is_some_and(|event| event.offset <= lexeme.span().start())
        {
            let event = terminal_owner_events[next_terminal_event];
            if active_len == 0 || active_buf[active_len - 1] != (event.kind, event.opener) {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            active_len -= 1;
            verified_buf[verified_len] = event;
            verified_len += 1;
            next_terminal_event += 1;
        }

        if matches!(lexeme.kind(), LexemeKind::Eof) && active_len != 0 {
            // 终止性 char/comment 根因会按 Lexer 的级联抑制规则取代外层 string / interpolation
            // 诊断；此时没有显式 owner event，但剩余 owner 仍必须在同一 EOF 关闭。
            if !terminal_error_at_eof {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            while active_len != 0 {
                active_len -= 1;
                let (kind, opener) = active_buf[active_len];
                verified_buf[verified_len] = TerminalOwnerEvent {
                    offset: source_len,
                    kind,
                    opener,
                };
                verified_len += 1;
            }
        }

        match lexeme.kind() {
            LexemeKind::Token(TokenKind::StringStart) => {
                active_buf[active_len] = (TerminalOwnerKind::String, lexeme.span().start());
                active_len += 1;
            }
            LexemeKind::Token(TokenKind::InterpolationStart) => {
                active_buf[active_len] = (TerminalOwnerKind::Interpolation, lexeme.span().start());
                active_len += 1;
            }
            LexemeKind::Token(TokenKind::StringEnd) => {
                if active_len == 0 || active_buf[active_len - 1].0 != TerminalOwnerKind::String {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                active_len -= 1;
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                if active_len == 0
                    || active_buf[active_len - 1].0 != TerminalOwnerKind::Interpolation
                {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                active_len -= 1;
            }
            LexemeKind::Invalid(_)
                if terminal_escape_spans
                    .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                    .is_ok() =>
            {
                if active_len == 0 || active_buf[active_len - 1].0 != TerminalOwnerKind::String {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                let (_, opener) = active_buf[active_len - 1];
                active_len -= 1;
                verified_buf[verified_len] = TerminalOwnerEvent {
                    offset: lexeme.span().end(),
                    kind: TerminalOwnerKind::String,
                    opener,
                };
                verified_len += 1;
            }
            _ => {}
        }
    }

    if next_terminal_event != terminal_owner_events.len() {
        return Err(ParserInternalError::InvalidLexemeStream);
    }
    Ok(verified_len)
}
