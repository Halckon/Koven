use super::*;

pub(super) const fn is_postfix_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(
            Symbol::Dot
                | Symbol::QuestionDot
                | Symbol::LeftParen
                | Symbol::LeftBracket
                | Symbol::BangBang
                | Symbol::Question
                | Symbol::ColonColon
        ))
    )
}

pub(super) const fn is_recovery_callee_ignored_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Trivia(_)
            | LexemeKind::Token(TokenKind::Symbol(Symbol::Question | Symbol::BangBang))
    )
}

pub(super) const fn is_recovery_callee_terminal_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(
            TokenKind::Identifier
                | TokenKind::IntegerLiteral(_)
                | TokenKind::FloatLiteral(_)
                | TokenKind::CharLiteral
                | TokenKind::StringEnd
                | TokenKind::Keyword(
                    Keyword::True | Keyword::False | Keyword::Null | Keyword::This
                )
                | TokenKind::Symbol(Symbol::RightParen | Symbol::RightBracket | Symbol::RightBrace)
        )
    )
}

pub(super) fn has_recovery_callee_end_in<T: Copy>(
    items: &[T],
    kind_of: impl Fn(T) -> LexemeKind,
) -> bool {
    let mut index = items.len();
    while index > 0 {
        index -= 1;
        let kind = kind_of(items[index]);
        if !is_recovery_callee_ignored_kind(kind) {
            return is_recovery_callee_terminal_kind(kind);
        }
    }
    false
}

#[cfg(any(test, kani))]
pub(super) fn has_recovery_callee_end_kinds(kinds: &[LexemeKind]) -> bool {
    has_recovery_callee_end_in(kinds, |kind| kind)
}

pub(super) const fn typed_call_suffix_continues_kind(
    suffix_kind: LexemeKind,
    has_line_break: bool,
) -> bool {
    !matches!(
        suffix_kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
    ) || !has_line_break
}

pub(super) const fn call_argument_boundary_decision(
    kind: LexemeKind,
    outer_contains: bool,
    can_start_expr: bool,
    at_block_separator: bool,
) -> bool {
    matches!(
        kind,
        LexemeKind::Eof | LexemeKind::Token(TokenKind::Symbol(Symbol::Comma | Symbol::RightParen))
    ) || (outer_contains && !can_start_expr)
        || at_block_separator
}

pub(super) const fn trailing_lambda_attaches_after_call(
    closed: bool,
    current_kind: LexemeKind,
    next_when_entry: bool,
) -> bool {
    closed
        && matches!(
            current_kind,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
        )
        && !next_when_entry
}
