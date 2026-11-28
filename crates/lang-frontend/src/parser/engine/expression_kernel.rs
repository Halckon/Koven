use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WhenConditionDispatch {
    TypeTest { negated: bool },
    Contains { negated: bool },
    Expression,
    ExpectedWhenEntry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GroupNonPoisonTailAction {
    AtRightParen,
    RecoverDeclarationTail,
    TryConfirmedCloser,
    NoSkip,
}

#[inline]
pub(super) const fn is_when_body_line_break_infix_stop_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(Keyword::Is | Keyword::In))
            | LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs | Symbol::BangIn))
    )
}

#[inline]
pub(super) const fn prefix_operator_for_kind(kind: LexemeKind) -> Option<PrefixOperator> {
    match kind {
        LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)) => Some(PrefixOperator::Not),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => Some(PrefixOperator::Plus),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => Some(PrefixOperator::Minus),
        _ => None,
    }
}

#[inline]
pub(super) const fn when_condition_dispatch(
    kind: LexemeKind,
    can_start_expression: bool,
) -> WhenConditionDispatch {
    match kind {
        LexemeKind::Token(TokenKind::Keyword(Keyword::Is)) => {
            WhenConditionDispatch::TypeTest { negated: false }
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)) => {
            WhenConditionDispatch::TypeTest { negated: true }
        }
        LexemeKind::Token(TokenKind::Keyword(Keyword::In)) => {
            WhenConditionDispatch::Contains { negated: false }
        }
        LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)) => {
            WhenConditionDispatch::Contains { negated: true }
        }
        _ if can_start_expression => WhenConditionDispatch::Expression,
        _ => WhenConditionDispatch::ExpectedWhenEntry,
    }
}

#[inline]
pub(super) const fn return_parses_value(
    can_start_expression: bool,
    has_line_break: bool,
    in_stops: bool,
    is_control_start: bool,
) -> bool {
    can_start_expression && !has_line_break && (!in_stops || is_control_start)
}

#[inline]
pub(super) fn is_group_declaration_tail_kind(kind: LexemeKind, file_mode: bool) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
    ) || (file_mode && file_construct_start_kind(kind))
}

#[inline]
pub(super) const fn group_non_poison_tail_action(
    at_right_paren: bool,
    declaration_tail: bool,
    at_comma: bool,
    in_outer_stops: bool,
) -> GroupNonPoisonTailAction {
    if at_right_paren {
        GroupNonPoisonTailAction::AtRightParen
    } else if declaration_tail {
        GroupNonPoisonTailAction::RecoverDeclarationTail
    } else if at_comma || !in_outer_stops {
        GroupNonPoisonTailAction::TryConfirmedCloser
    } else {
        GroupNonPoisonTailAction::NoSkip
    }
}

#[inline]
pub(super) const fn string_missing_closer_emits_diagnostic(
    lexer_owns_owner_recovery: bool,
    at_source_end: bool,
    terminal_error_at_eof: bool,
) -> bool {
    !(lexer_owns_owner_recovery || (at_source_end && terminal_error_at_eof))
}
