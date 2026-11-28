use super::lambda::{
    LambdaBodyLoopAction, is_lambda_body_unsupported_symbol_kind, lambda_body_loop_action,
    lambda_body_missing_closer_emits_diagnostic, skip_lambda_body_semicolons_in,
    unsupported_lambda_body_consumes_val_kind,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind};

#[test]
fn lambda_body_loop_action_and_semicolon_skip_match_spec() {
    assert_eq!(
        lambda_body_loop_action(
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)),
            false
        ),
        LambdaBodyLoopAction::Stop
    );
    assert_eq!(
        lambda_body_loop_action(
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            true
        ),
        LambdaBodyLoopAction::Stop
    );
    assert_eq!(
        lambda_body_loop_action(
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            false
        ),
        LambdaBodyLoopAction::SkipSemicolon
    );
    assert_eq!(
        lambda_body_loop_action(LexemeKind::Token(TokenKind::Identifier), false),
        LambdaBodyLoopAction::ParseElement
    );

    let items = [
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            false,
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            false,
        ),
        (LexemeKind::Token(TokenKind::Identifier), false),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            false,
        ),
    ];
    assert_eq!(skip_lambda_body_semicolons_in(&items, |x| x.0, |x| x.1), 2);
    let hard_stop_semi = [
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            false,
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
            true,
        ),
    ];
    assert_eq!(
        skip_lambda_body_semicolons_in(&hard_stop_semi, |x| x.0, |x| x.1),
        1
    );
}

#[test]
fn lambda_body_unsupported_symbol_and_const_val_match_spec() {
    assert!(is_lambda_body_unsupported_symbol_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::Comma)
    )));
    assert!(is_lambda_body_unsupported_symbol_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::Arrow)
    )));
    assert!(!is_lambda_body_unsupported_symbol_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::Semicolon)
    )));

    assert!(unsupported_lambda_body_consumes_val_kind(
        LexemeKind::Token(TokenKind::Keyword(Keyword::Const)),
        Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Val)))
    ));
    assert!(!unsupported_lambda_body_consumes_val_kind(
        LexemeKind::Token(TokenKind::Keyword(Keyword::Const)),
        Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Var)))
    ));
    assert!(!unsupported_lambda_body_consumes_val_kind(
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val)),
        Some(LexemeKind::Token(TokenKind::Keyword(Keyword::Val)))
    ));
}

#[test]
fn lambda_body_missing_closer_guard_respects_brace_eof_and_poison() {
    assert!(!lambda_body_missing_closer_emits_diagnostic(
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)),
        false,
        false
    ));
    assert!(lambda_body_missing_closer_emits_diagnostic(
        LexemeKind::Eof,
        false,
        false
    ));
    assert!(!lambda_body_missing_closer_emits_diagnostic(
        LexemeKind::Eof,
        true,
        false
    ));
    assert!(!lambda_body_missing_closer_emits_diagnostic(
        LexemeKind::Eof,
        false,
        true
    ));
}
