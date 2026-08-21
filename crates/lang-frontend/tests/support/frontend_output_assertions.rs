//! Lexer / Parser matrix shared source-local output assertions.

use lang_frontend::{parser::SyntaxAst, source::SourceId};

#[path = "lexer_output_assertions.rs"]
mod lexer_output_assertions;

pub(crate) use lexer_output_assertions::{validate_diagnostics, validate_lexed, validate_span};

pub(crate) fn validate_ast(source_id: SourceId, source_len: usize, ast: &SyntaxAst) {
    assert_eq!(ast.source_id(), source_id);
    for (_, node) in ast.items().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.statements().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.expressions().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.type_refs().iter() {
        validate_span(source_id, source_len, node.span());
    }
}
