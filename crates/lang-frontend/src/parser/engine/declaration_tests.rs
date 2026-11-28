use super::ParameterModeContext;
use super::declaration::{
    ConstValRecoveryAction, FunctionFormDispatch, ValueParamTypeAction, const_val_recovery_action,
    declaration_tail_emits_unexpected_trailing, function_form_dispatch,
    is_parameter_mode_follow_kind, value_param_default_emits_diagnostic, value_param_type_action,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind};

#[test]
fn const_val_recovery_and_function_form_dispatch_match_spec() {
    let var = LexemeKind::Token(TokenKind::Keyword(Keyword::Var));
    let ident = LexemeKind::Token(TokenKind::Identifier);
    let str_start = LexemeKind::Token(TokenKind::StringStart);
    let colon = LexemeKind::Token(TokenKind::Symbol(Symbol::Colon));
    let equal = LexemeKind::Token(TokenKind::Symbol(Symbol::Equal));
    let lbrace = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace));

    assert_eq!(
        const_val_recovery_action(var, true),
        ConstValRecoveryAction::MissingAtBoundary
    );
    assert_eq!(
        const_val_recovery_action(var, false),
        ConstValRecoveryAction::ConsumeErrorVar
    );
    for keep in [ident, str_start, colon, equal] {
        assert_eq!(
            const_val_recovery_action(keep, false),
            ConstValRecoveryAction::MissingKeepCurrent
        );
    }
    assert_eq!(
        const_val_recovery_action(lbrace, false),
        ConstValRecoveryAction::ConsumeErrorOther
    );

    assert_eq!(
        function_form_dispatch(colon, true),
        FunctionFormDispatch::ExplicitColon
    );
    assert_eq!(
        function_form_dispatch(equal, true),
        FunctionFormDispatch::MissingReturnTypeBeforeEqual
    );
    assert_eq!(
        function_form_dispatch(lbrace, true),
        FunctionFormDispatch::ImplicitUnitBlock
    );
    assert_eq!(
        function_form_dispatch(ident, true),
        FunctionFormDispatch::MissingColonBeforeTypeRef
    );
    assert_eq!(
        function_form_dispatch(ident, false),
        FunctionFormDispatch::ImplicitUnitAbsent
    );
}

#[test]
fn parameter_mode_follow_and_value_param_guards_match_spec() {
    let ident = LexemeKind::Token(TokenKind::Identifier);
    let lparen = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen));
    let rparen = LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen));
    let equal = LexemeKind::Token(TokenKind::Symbol(Symbol::Equal));
    let comma = LexemeKind::Token(TokenKind::Symbol(Symbol::Comma));

    assert!(is_parameter_mode_follow_kind(
        ParameterModeContext::NamedParameter,
        ident
    ));
    assert!(!is_parameter_mode_follow_kind(
        ParameterModeContext::NamedParameter,
        lparen
    ));
    assert!(is_parameter_mode_follow_kind(
        ParameterModeContext::FunctionType,
        ident
    ));
    assert!(is_parameter_mode_follow_kind(
        ParameterModeContext::FunctionType,
        lparen
    ));
    assert!(!is_parameter_mode_follow_kind(
        ParameterModeContext::FunctionType,
        rparen
    ));

    for boundary in [equal, comma, rparen, LexemeKind::Eof] {
        assert_eq!(
            value_param_type_action(boundary, true),
            ValueParamTypeAction::EmptyErrorAtBoundary
        );
    }
    assert_eq!(
        value_param_type_action(ident, true),
        ValueParamTypeAction::ParseTypeRef
    );
    assert_eq!(
        value_param_type_action(ident, false),
        ValueParamTypeAction::RecoverErrorRegion
    );

    // `fun f(x = 1)` suppresses duplicate default diagnostic, whereas `fun f(x Int = 1)` still emits it.
    assert!(!value_param_default_emits_diagnostic(true, true, true));
    assert!(value_param_default_emits_diagnostic(false, true, true));
    assert!(value_param_default_emits_diagnostic(true, false, true));
    assert!(!value_param_default_emits_diagnostic(false, false, false));
}

#[test]
fn declaration_tail_guard_respects_eof_and_implicit_function_poison() {
    let ident = LexemeKind::Token(TokenKind::Identifier);
    assert!(!declaration_tail_emits_unexpected_trailing(
        LexemeKind::Eof,
        false,
        false
    ));
    assert!(!declaration_tail_emits_unexpected_trailing(
        ident, true, true
    ));
    assert!(declaration_tail_emits_unexpected_trailing(
        ident, true, false
    ));
    assert!(declaration_tail_emits_unexpected_trailing(
        ident, false, true
    ));
}
