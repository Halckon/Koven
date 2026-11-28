use super::lambda::{
    LambdaBodyLoopAction, is_lambda_body_unsupported_symbol_kind, lambda_body_loop_action,
    lambda_body_missing_closer_emits_diagnostic, skip_lambda_body_semicolons_with,
    unsupported_lambda_body_consumes_val_kind,
};
use super::{Expression, Statement, parse};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind, lex_test_source_twice};
use crate::source::SourceMap;

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
    let mut pos = 0usize;
    let outcome = skip_lambda_body_semicolons_with(
        &mut pos,
        |idx| items.get(*idx).copied().ok_or(()),
        |idx| {
            *idx += 1;
            Ok(())
        },
        |x| x.0,
        |x| x.1,
    );
    assert_eq!(pos, 2);
    assert_eq!(
        outcome,
        Ok((
            (LexemeKind::Token(TokenKind::Identifier), false),
            LambdaBodyLoopAction::ParseElement
        ))
    );

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
    let mut hard_pos = 0usize;
    let hard_outcome = skip_lambda_body_semicolons_with(
        &mut hard_pos,
        |idx| hard_stop_semi.get(*idx).copied().ok_or(()),
        |idx| {
            *idx += 1;
            Ok(())
        },
        |x| x.0,
        |x| x.1,
    );
    assert_eq!(hard_pos, 1);
    assert_eq!(
        hard_outcome,
        Ok((
            (
                LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
                true
            ),
            LambdaBodyLoopAction::Stop
        ))
    );
}

#[test]
fn lambda_body_production_entry_skips_consecutive_semicolons_and_parses_element() {
    for text in ["{ ;; x }", "{ ; ; x ; }"] {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("lambda-semicolons.ko", text)
            .expect("test source name must be unique");
        let lexed = lex_test_source_twice(&sources, source_id, "lambda consecutive semicolons");
        let parsed = parse(&sources, &lexed).expect("lambda must parse");
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?} unexpectedly emitted diagnostics: {:?}",
            parsed.diagnostics()
        );
        let Expression::Lambda { body, .. } = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("lambda root")
            .payload()
        else {
            panic!("{text:?}: expected Expression::Lambda");
        };
        let Statement::LambdaBody { elements } = parsed
            .ast()
            .statements()
            .get(*body)
            .expect("lambda body")
            .payload()
        else {
            panic!("{text:?}: expected Statement::LambdaBody");
        };
        assert_eq!(elements.len(), 1, "{text:?}: expected single element");
        let Statement::Expression { expression } = parsed
            .ast()
            .statements()
            .get(elements[0])
            .expect("body element")
            .payload()
        else {
            panic!("{text:?}: expected Statement::Expression");
        };
        let expr_node = parsed
            .ast()
            .expressions()
            .get(*expression)
            .expect("body expression");
        assert!(matches!(expr_node.payload(), Expression::Name));
        assert_eq!(
            sources.slice(expr_node.span()).expect("expression slice"),
            "x"
        );
    }
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
