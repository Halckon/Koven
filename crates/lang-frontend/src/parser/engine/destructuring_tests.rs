use super::destructuring::{
    DestructuringEvent, DestructuringExitKind, DestructuringLoopState,
    destructuring_exit_emits_missing_binding, destructuring_initializer_missing_diagnostic,
    is_const_local_destructuring_start_kinds, is_local_destructuring_start_kinds,
    run_destructuring_loop,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind};

#[test]
fn destructuring_start_predicates_match_exact_prefixes() {
    let val = LexemeKind::Token(TokenKind::Keyword(Keyword::Val));
    let kw_const = LexemeKind::Token(TokenKind::Keyword(Keyword::Const));
    let lparen = Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)));
    assert!(is_local_destructuring_start_kinds(
        Keyword::Val,
        val,
        lparen
    ));
    assert!(!is_local_destructuring_start_kinds(
        Keyword::Var,
        val,
        lparen
    ));
    assert!(is_const_local_destructuring_start_kinds(
        kw_const,
        Some(val),
        lparen
    ));
    assert!(!is_const_local_destructuring_start_kinds(
        val,
        Some(val),
        lparen
    ));
}

#[test]
fn destructuring_loop_prevents_double_missing_binding_after_empty_comma() {
    let state = run_destructuring_loop(
        DestructuringLoopState::new(),
        &[DestructuringEvent::EmptyComma],
    );
    assert!(!destructuring_exit_emits_missing_binding(
        DestructuringExitKind::RightParen,
        state
    ));
    assert!(!destructuring_exit_emits_missing_binding(
        DestructuringExitKind::Boundary,
        state
    ));

    let after_sep = run_destructuring_loop(
        DestructuringLoopState::new(),
        &[
            DestructuringEvent::Binding,
            DestructuringEvent::SeparatorComma,
        ],
    );
    assert!(destructuring_exit_emits_missing_binding(
        DestructuringExitKind::Boundary,
        after_sep
    ));
}

#[test]
fn destructuring_initializer_diagnostic_respects_primary_poison_and_eof_guards() {
    assert!(destructuring_initializer_missing_diagnostic(
        true, true, false, false, false
    ));
    assert!(!destructuring_initializer_missing_diagnostic(
        true, true, true, false, false
    ));
    assert!(!destructuring_initializer_missing_diagnostic(
        true, true, false, true, false
    ));
    assert!(!destructuring_initializer_missing_diagnostic(
        true, true, false, false, true
    ));
}
