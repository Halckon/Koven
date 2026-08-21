//! Core Parser integration tests shared repeated-output assertions.

#![allow(
    dead_code,
    reason = "typed wrappers are compiled separately by their integration targets"
)]

use lang_frontend::{
    lexer::LexedFile,
    parser::{
        ParsedBlock, ParsedDeclaration, ParsedExpression, parse_block, parse_declaration,
        parse_expression,
    },
    source::{SourceId, SourceMap},
};

#[path = "frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use lexer_matrix_assertions::lex_loaded_source_twice;

pub(crate) fn parse_expression_twice(
    sources: &SourceMap,
    source_id: SourceId,
    context: &str,
) -> ParsedExpression {
    let (source_len, lexed) = parser_source_twice(sources, source_id, context);
    parse_expression_from_lexed_twice(sources, source_id, source_len, &lexed, context)
}

pub(crate) fn parse_declaration_twice(
    sources: &SourceMap,
    source_id: SourceId,
    context: &str,
) -> ParsedDeclaration {
    let (source_len, lexed) = parser_source_twice(sources, source_id, context);
    parse_declaration_from_lexed_twice(sources, source_id, source_len, &lexed, context)
}

pub(crate) fn parse_block_twice(
    sources: &SourceMap,
    source_id: SourceId,
    context: &str,
) -> ParsedBlock {
    let (source_len, lexed) = parser_source_twice(sources, source_id, context);
    parse_block_from_lexed_twice(sources, source_id, source_len, &lexed, context)
}

fn parser_source_twice(
    sources: &SourceMap,
    source_id: SourceId,
    context: &str,
) -> (usize, LexedFile) {
    let source_len = sources
        .source_text(source_id)
        .unwrap_or_else(|error| panic!("source lookup failed for {context}: {error}"))
        .len();
    let lexed = lex_loaded_source_twice(sources, source_id, source_len, context, validate_lexed);
    (source_len, lexed)
}

fn parse_expression_from_lexed_twice(
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
    context: &str,
) -> ParsedExpression {
    let first = parse_expression(sources, lexed)
        .unwrap_or_else(|error| panic!("first expression parse failed for {context}: {error}"));
    let repeated = parse_expression(sources, lexed)
        .unwrap_or_else(|error| panic!("repeated expression parse failed for {context}: {error}"));
    for parsed in [&first, &repeated] {
        assert_eq!(parsed.source_id(), source_id);
        validate_ast(source_id, source_len, parsed.ast());
        validate_diagnostics(source_id, source_len, parsed.diagnostics());
        parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .unwrap_or_else(|error| panic!("expression root failed for {context}: {error}"));
    }
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic expression parse for {context}"
    );
    first
}

fn parse_declaration_from_lexed_twice(
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
    context: &str,
) -> ParsedDeclaration {
    let first = parse_declaration(sources, lexed)
        .unwrap_or_else(|error| panic!("first declaration parse failed for {context}: {error}"));
    let repeated = parse_declaration(sources, lexed)
        .unwrap_or_else(|error| panic!("repeated declaration parse failed for {context}: {error}"));
    for parsed in [&first, &repeated] {
        assert_eq!(parsed.source_id(), source_id);
        validate_ast(source_id, source_len, parsed.ast());
        validate_diagnostics(source_id, source_len, parsed.diagnostics());
        parsed
            .ast()
            .items()
            .get(parsed.root())
            .unwrap_or_else(|error| panic!("declaration root failed for {context}: {error}"));
    }
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic declaration parse for {context}"
    );
    first
}

fn parse_block_from_lexed_twice(
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
    context: &str,
) -> ParsedBlock {
    let first = parse_block(sources, lexed)
        .unwrap_or_else(|error| panic!("first block parse failed for {context}: {error}"));
    let repeated = parse_block(sources, lexed)
        .unwrap_or_else(|error| panic!("repeated block parse failed for {context}: {error}"));
    for parsed in [&first, &repeated] {
        assert_eq!(parsed.source_id(), source_id);
        validate_ast(source_id, source_len, parsed.ast());
        validate_diagnostics(source_id, source_len, parsed.diagnostics());
        parsed
            .ast()
            .statements()
            .get(parsed.root())
            .unwrap_or_else(|error| panic!("block root failed for {context}: {error}"));
    }
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic block parse for {context}"
    );
    first
}
