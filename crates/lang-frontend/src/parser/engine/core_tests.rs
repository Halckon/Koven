use super::core::{
    boundary_uses_empty_span, control_body_element_is_statement,
    expression_tail_emits_unexpected_trailing, expression_tail_is_block_boundary,
    if_missing_else_emits_diagnostic, lambda_body_element_is_statement,
    previous_significant_end_with, try_enter_recursion_depth, type_recovery_boundary_hit,
};
use super::{parse, parse_block};
use crate::diagnostic::codes;
use crate::lexer::{LexemeKind, Symbol, TokenKind, TriviaKind, lex_test_source_twice};
use crate::source::SourceMap;

#[test]
fn recursion_depth_and_previous_significant_end_match_spec() {
    assert_eq!(try_enter_recursion_depth(0, 64), Some(1));
    assert_eq!(try_enter_recursion_depth(63, 64), Some(64));
    assert_eq!(try_enter_recursion_depth(64, 64), None);
    assert_eq!(try_enter_recursion_depth(100, 64), None);
    assert_eq!(try_enter_recursion_depth(0, 0), None);

    let items = [
        (LexemeKind::Trivia(TriviaKind::Whitespace), 2usize),
        (LexemeKind::Token(TokenKind::Identifier), 5),
        (LexemeKind::Trivia(TriviaKind::Newline), 6),
        (LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)), 7),
        (LexemeKind::Trivia(TriviaKind::LineComment), 12),
    ];
    assert_eq!(
        previous_significant_end_with(&items[..0], |x| x.0, |x| x.1),
        0
    );
    assert_eq!(
        previous_significant_end_with(&items[..1], |x| x.0, |x| x.1),
        0
    );
    assert_eq!(
        previous_significant_end_with(&items[..3], |x| x.0, |x| x.1),
        5
    );
    assert_eq!(previous_significant_end_with(&items, |x| x.0, |x| x.1), 7);
}

#[test]
fn body_context_propagation_and_if_else_guard_match_spec() {
    assert!(!lambda_body_element_is_statement(2, 3, true));
    assert!(lambda_body_element_is_statement(1, 3, true));
    assert!(lambda_body_element_is_statement(2, 3, false));

    assert!(!control_body_element_is_statement(true, 1, 2, true));
    assert!(control_body_element_is_statement(false, 1, 2, true));
    assert!(control_body_element_is_statement(true, 0, 2, true));
    assert!(control_body_element_is_statement(true, 1, 2, false));

    assert!(if_missing_else_emits_diagnostic(false, false));
    assert!(!if_missing_else_emits_diagnostic(true, false));
    assert!(!if_missing_else_emits_diagnostic(false, true));
    assert!(!if_missing_else_emits_diagnostic(true, true));
}

#[test]
fn expression_tail_and_recovery_boundary_guards_match_spec() {
    assert!(expression_tail_is_block_boundary(true, true, false));
    assert!(expression_tail_is_block_boundary(true, false, true));
    assert!(!expression_tail_is_block_boundary(false, true, true));
    assert!(!expression_tail_is_block_boundary(true, false, false));

    assert!(expression_tail_emits_unexpected_trailing(false, false));
    assert!(!expression_tail_emits_unexpected_trailing(true, false));
    assert!(!expression_tail_emits_unexpected_trailing(false, true));

    assert!(boundary_uses_empty_span(true, false));
    assert!(boundary_uses_empty_span(false, true));
    assert!(!boundary_uses_empty_span(false, false));

    assert!(!type_recovery_boundary_hit(
        false, false, true, false, true, false
    ));
    assert!(type_recovery_boundary_hit(
        false, true, true, false, false, false
    ));
    assert!(type_recovery_boundary_hit(
        false, false, false, true, true, false
    ));
    assert!(type_recovery_boundary_hit(
        false, false, false, false, false, true
    ));
}

#[test]
fn core_production_entry_enforces_if_else_context_in_value_and_statement_positions() {
    // In value position (expression entry), `if (cond) { a }` without `else` must emit EXPECTED_ELSE_BRANCH.
    let mut sources = SourceMap::new();
    let value_id = sources
        .add_source("if-value.ko", "if (cond) { a }")
        .expect("unique source");
    let value_lexed = lex_test_source_twice(&sources, value_id, "if value position");
    let value_parsed = parse(&sources, &value_lexed).expect("expression must parse");
    assert_eq!(value_parsed.diagnostics().len(), 1);
    assert_eq!(
        value_parsed.diagnostics()[0].code().to_string(),
        codes::EXPECTED_ELSE_BRANCH
    );

    // In statement position (block entry), `if (cond) { a }` without `else` is valid.
    let block_id = sources
        .add_source("if-stmt.ko", "{ if (cond) { a } }")
        .expect("unique source");
    let block_lexed = lex_test_source_twice(&sources, block_id, "if statement position");
    let block_parsed = parse_block(&sources, &block_lexed).expect("block must parse");
    assert!(
        block_parsed.diagnostics().is_empty(),
        "statement-position if without else must not emit diagnostics: {:?}",
        block_parsed.diagnostics()
    );

    // Trailing unexpected token in expression entry must emit UNEXPECTED_TRAILING_TOKEN.
    let tail_id = sources
        .add_source("expr-tail.ko", "a )")
        .expect("unique source");
    let tail_lexed = lex_test_source_twice(&sources, tail_id, "expression tail");
    let tail_parsed = parse(&sources, &tail_lexed).expect("expression must parse");
    assert!(
        tail_parsed
            .diagnostics()
            .iter()
            .any(|diag| diag.code().to_string() == codes::UNEXPECTED_TRAILING_TOKEN),
        "expected UNEXPECTED_TRAILING_TOKEN, got {:?}",
        tail_parsed.diagnostics()
    );
}
