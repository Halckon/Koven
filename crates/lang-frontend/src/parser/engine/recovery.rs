use super::*;

pub(super) struct LexicalRecoveryIndex {
    pub(super) source_len: usize,
    string_owner_ends: Vec<(usize, usize)>,
    string_recoveries: Vec<(usize, usize)>,
    lexical_poison_string_recoveries: Vec<(usize, usize)>,
    pub(super) unterminated_interpolation_starts: Vec<usize>,
    pub(super) terminal_error_at_eof: bool,
    pub(super) terminal_owner_events: Vec<TerminalOwnerEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::parser) enum TerminalOwnerKind {
    String,
    Interpolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::parser) struct TerminalOwnerEvent {
    pub(in crate::parser) offset: usize,
    pub(in crate::parser) kind: TerminalOwnerKind,
    pub(in crate::parser) opener: usize,
}

#[derive(Clone, Copy)]
enum LexicalDiagnosticAnchor {
    ExactInvalid(InvalidKind),
    ExactReservedWord,
    StringStart,
    InterpolationStart,
}

impl LexicalRecoveryIndex {
    pub(super) fn new(source: &str, lexed: &LexedFile) -> Result<Self, ParserInternalError> {
        let catalog = codes::catalog()?;
        let unexpected_character = catalog.resolve(codes::UNEXPECTED_CHARACTER)?;
        let reserved_word = catalog.resolve(codes::RESERVED_WORD)?;
        let unterminated_string = catalog.resolve(codes::UNTERMINATED_STRING)?;
        let unterminated_interpolation = catalog.resolve(codes::UNTERMINATED_INTERPOLATION)?;
        let invalid_string_escape = catalog.resolve(codes::INVALID_STRING_ESCAPE)?;
        let invalid_char_literal = catalog.resolve(codes::INVALID_CHAR_LITERAL)?;
        let unterminated_block_comment = catalog.resolve(codes::UNTERMINATED_BLOCK_COMMENT)?;
        let invalid_numeric_literal = catalog.resolve(codes::INVALID_NUMERIC_LITERAL)?;

        let mut string_recoveries = Vec::new();
        let mut string_exit_events = Vec::new();
        let mut invalid_string_escape_spans = Vec::new();
        let mut terminal_escape_spans = Vec::new();
        let mut terminal_other_spans = Vec::new();
        let mut unterminated_interpolation_starts = Vec::new();
        let mut terminal_error_at_eof = false;
        let mut terminal_owner_events = Vec::new();
        let mut diagnosed_lexemes = vec![false; lexed.lexemes().len()];

        for diagnostic in lexed.diagnostics() {
            let span = diagnostic.primary_span();
            let code = diagnostic.code();
            let anchor = if code == unexpected_character {
                LexicalDiagnosticAnchor::ExactInvalid(InvalidKind::UnexpectedCharacter)
            } else if code == reserved_word {
                LexicalDiagnosticAnchor::ExactReservedWord
            } else if code == unterminated_block_comment {
                LexicalDiagnosticAnchor::ExactInvalid(InvalidKind::UnterminatedBlockComment)
            } else if code == unterminated_string {
                LexicalDiagnosticAnchor::StringStart
            } else if code == unterminated_interpolation {
                LexicalDiagnosticAnchor::InterpolationStart
            } else if code == invalid_string_escape {
                LexicalDiagnosticAnchor::ExactInvalid(InvalidKind::InvalidStringEscape)
            } else if code == invalid_char_literal {
                LexicalDiagnosticAnchor::ExactInvalid(InvalidKind::InvalidCharLiteral)
            } else if code == invalid_numeric_literal {
                LexicalDiagnosticAnchor::ExactInvalid(InvalidKind::InvalidNumericLiteral)
            } else {
                return Err(ParserInternalError::InvalidLexemeStream);
            };
            if span.source_id() != lexed.source_id() {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            let Some(anchor_index) = diagnostic_anchor_index(lexed, span, anchor) else {
                return Err(ParserInternalError::InvalidLexemeStream);
            };
            if diagnosed_lexemes[anchor_index] {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            diagnosed_lexemes[anchor_index] = true;
            if code == unterminated_string {
                string_recoveries.push((span.start(), span.end()));
                string_exit_events.push((span.end(), span.start()));
                terminal_error_at_eof |= span.end() == source.len();
                terminal_owner_events.push(TerminalOwnerEvent {
                    offset: span.end(),
                    kind: TerminalOwnerKind::String,
                    opener: span.start(),
                });
                continue;
            }
            if code == unterminated_interpolation {
                unterminated_interpolation_starts.push(span.start());
                terminal_error_at_eof |= span.end() == source.len();
                terminal_owner_events.push(TerminalOwnerEvent {
                    offset: span.end(),
                    kind: TerminalOwnerKind::Interpolation,
                    opener: span.start(),
                });
                continue;
            }
            if code == invalid_string_escape {
                invalid_string_escape_spans.push((span.start(), span.end()));
                if span.end() == span.start() + 1
                    && (span.end() == source.len()
                        || source[span.end()..].starts_with(['\r', '\n']))
                {
                    terminal_escape_spans.push((span.start(), span.end()));
                    terminal_error_at_eof |= span.end() == source.len();
                }
                continue;
            }
            let terminal_other = (code == unterminated_block_comment && span.end() == source.len())
                || (code == invalid_char_literal && unterminated_char_at_eof(source, span));
            if terminal_other {
                terminal_other_spans.push((span.start(), span.end()));
                terminal_error_at_eof |= span.end() == source.len();
            }
        }
        if lexed.lexemes().iter().enumerate().any(|(index, lexeme)| {
            matches!(
                lexeme.kind(),
                LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_))
            ) && !diagnosed_lexemes[index]
        }) {
            return Err(ParserInternalError::InvalidLexemeStream);
        }

        string_exit_events.sort_unstable();
        invalid_string_escape_spans.sort_unstable();
        terminal_escape_spans.sort_unstable();
        terminal_other_spans.sort_unstable();
        unterminated_interpolation_starts.sort_unstable();

        terminal_owner_events.sort_unstable_by(|left, right| {
            left.offset
                .cmp(&right.offset)
                .then_with(|| right.opener.cmp(&left.opener))
        });
        let mut verified_events = Vec::with_capacity(terminal_owner_events.len());
        let mut active_owners = Vec::new();
        let mut next_terminal_event = 0usize;
        for lexeme in lexed.lexemes() {
            while terminal_owner_events
                .get(next_terminal_event)
                .is_some_and(|event| event.offset <= lexeme.span().start())
            {
                let event = terminal_owner_events[next_terminal_event];
                if active_owners.last().copied() != Some((event.kind, event.opener)) {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                active_owners.pop();
                verified_events.push(event);
                next_terminal_event += 1;
            }

            if matches!(lexeme.kind(), LexemeKind::Eof) && !active_owners.is_empty() {
                // 终止性 char/comment 根因会按 Lexer 的级联抑制规则取代外层 string / interpolation
                // 诊断；此时没有显式 owner event，但剩余 owner 仍必须在同一 EOF 关闭。
                if !terminal_error_at_eof {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                while let Some((kind, opener)) = active_owners.pop() {
                    verified_events.push(TerminalOwnerEvent {
                        offset: source.len(),
                        kind,
                        opener,
                    });
                }
            }

            match lexeme.kind() {
                LexemeKind::Token(TokenKind::StringStart) => {
                    active_owners.push((TerminalOwnerKind::String, lexeme.span().start()))
                }
                LexemeKind::Token(TokenKind::InterpolationStart) => {
                    active_owners.push((TerminalOwnerKind::Interpolation, lexeme.span().start()))
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    if !matches!(active_owners.pop(), Some((TerminalOwnerKind::String, _))) {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                }
                LexemeKind::Token(TokenKind::InterpolationEnd) => {
                    if !matches!(
                        active_owners.pop(),
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
                    let Some((TerminalOwnerKind::String, opener)) = active_owners.last().copied()
                    else {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    };
                    let event = TerminalOwnerEvent {
                        offset: lexeme.span().end(),
                        kind: TerminalOwnerKind::String,
                        opener,
                    };
                    active_owners.pop();
                    verified_events.push(event);
                }
                _ => {}
            }
        }
        if next_terminal_event != terminal_owner_events.len() {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        let mut active_strings: Vec<ActiveString> = Vec::new();
        let mut string_owner_ends = Vec::new();
        let mut lexical_poison_string_recoveries = Vec::new();
        let mut next_exit = 0usize;
        for lexeme in lexed.lexemes() {
            while string_exit_events
                .get(next_exit)
                .is_some_and(|(end, _)| *end <= lexeme.span().start())
            {
                let (_, owner) = string_exit_events[next_exit];
                for active_owner in &mut active_strings {
                    active_owner.lexical_poison = true;
                }
                if lexeme.span().start() == source.len() {
                    for active_owner in &active_strings {
                        string_recoveries.push((active_owner.opener, source.len()));
                    }
                }
                pop_string_owner(&mut active_strings, owner)?;
                next_exit += 1;
            }

            match lexeme.kind() {
                LexemeKind::Token(TokenKind::StringStart) => {
                    active_strings.push(ActiveString {
                        opener: lexeme.span().start(),
                        lexical_poison: false,
                    });
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    let owner = active_strings
                        .pop()
                        .ok_or(ParserInternalError::InvalidLexemeStream)?;
                    string_owner_ends.push((owner.opener, lexeme.span().end()));
                    if owner.lexical_poison {
                        lexical_poison_string_recoveries.push((owner.opener, lexeme.span().end()));
                    }
                }
                LexemeKind::Token(TokenKind::InterpolationStart)
                    if unterminated_interpolation_starts
                        .binary_search(&lexeme.span().start())
                        .is_ok() =>
                {
                    for owner in &active_strings {
                        string_recoveries.push((owner.opener, source.len()));
                    }
                }
                LexemeKind::Invalid(_)
                    if invalid_string_escape_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_ok() =>
                {
                    if active_strings.is_empty() {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                    for owner in &mut active_strings {
                        owner.lexical_poison = true;
                    }
                    if terminal_escape_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_err()
                    {
                        continue;
                    }
                    let end = lexeme.span().end();
                    if end == source.len() {
                        for owner in &active_strings {
                            string_recoveries.push((owner.opener, end));
                        }
                    } else if let Some(owner) = active_strings.last() {
                        string_recoveries.push((owner.opener, end));
                    }
                    if let Some(owner) = active_strings.last().copied() {
                        pop_string_owner(&mut active_strings, owner.opener)?;
                    }
                }
                LexemeKind::Invalid(_)
                    if terminal_other_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_ok() =>
                {
                    for owner in &active_strings {
                        string_recoveries.push((owner.opener, source.len()));
                    }
                }
                _ => {}
            }
        }

        string_recoveries.sort_unstable();
        string_recoveries.dedup_by_key(|(start, _)| *start);
        string_owner_ends.sort_unstable();
        string_owner_ends.dedup_by_key(|(start, _)| *start);
        lexical_poison_string_recoveries.sort_unstable();
        lexical_poison_string_recoveries.dedup_by_key(|(start, _)| *start);
        Ok(Self {
            source_len: source.len(),
            string_owner_ends,
            string_recoveries,
            lexical_poison_string_recoveries,
            unterminated_interpolation_starts,
            terminal_error_at_eof,
            terminal_owner_events: verified_events,
        })
    }

    pub(super) fn string_recovery_end(&self, start: usize) -> Option<usize> {
        self.string_recoveries
            .binary_search_by_key(&start, |(owner, _)| *owner)
            .ok()
            .map(|index| self.string_recoveries[index].1)
    }

    pub(super) fn string_owner_end(&self, start: usize) -> Option<usize> {
        self.string_owner_ends
            .binary_search_by_key(&start, |(owner, _)| *owner)
            .ok()
            .map(|index| self.string_owner_ends[index].1)
    }

    pub(super) fn lexical_poison_string_recovery_end(&self, start: usize) -> Option<usize> {
        self.lexical_poison_string_recoveries
            .binary_search_by_key(&start, |(owner, _)| *owner)
            .ok()
            .map(|index| self.lexical_poison_string_recoveries[index].1)
    }
}

fn diagnostic_anchor_index(
    lexed: &LexedFile,
    span: Span,
    expected: LexicalDiagnosticAnchor,
) -> Option<usize> {
    let Ok(index) = lexed
        .lexemes()
        .binary_search_by_key(&span.start(), |lexeme| lexeme.span().start())
    else {
        return None;
    };
    let lexeme = lexed.lexemes()[index];
    let matches = match expected {
        LexicalDiagnosticAnchor::ExactInvalid(kind) => {
            lexeme.span() == span && lexeme.kind() == LexemeKind::Invalid(kind)
        }
        LexicalDiagnosticAnchor::ExactReservedWord => {
            lexeme.span() == span
                && matches!(lexeme.kind(), LexemeKind::Token(TokenKind::ReservedWord(_)))
        }
        LexicalDiagnosticAnchor::StringStart => {
            lexeme.span().start() == span.start()
                && lexeme.kind() == LexemeKind::Token(TokenKind::StringStart)
        }
        LexicalDiagnosticAnchor::InterpolationStart => {
            lexeme.kind() == LexemeKind::Token(TokenKind::InterpolationStart)
        }
    };
    matches.then_some(index)
}

#[derive(Clone, Copy)]
struct ActiveString {
    opener: usize,
    lexical_poison: bool,
}

fn pop_string_owner(
    active_strings: &mut Vec<ActiveString>,
    owner: usize,
) -> Result<(), ParserInternalError> {
    if active_strings.last().map(|active| active.opener) != Some(owner) {
        return Err(ParserInternalError::InvalidLexemeStream);
    }
    active_strings.pop();
    Ok(())
}

fn unterminated_char_at_eof(source: &str, span: Span) -> bool {
    if span.end() != source.len() {
        return false;
    }
    let mut chars = source[span.start()..span.end()].chars();
    if chars.next() != Some('\'') {
        return false;
    }
    while let Some(character) = chars.next() {
        match character {
            '\'' => return false,
            '\r' | '\n' => return false,
            '\\' => match chars.next() {
                Some('\r' | '\n') => return false,
                Some(_) => {}
                None => return true,
            },
            _ => {}
        }
    }
    true
}
