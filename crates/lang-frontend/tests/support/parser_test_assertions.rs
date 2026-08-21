//! Core Parser integration tests shared repeated-output assertions.

use lang_frontend::{
    lexer::LexedFile,
    parser::{ParsedExpression, parse_expression},
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
    let source_len = sources
        .source_text(source_id)
        .unwrap_or_else(|error| panic!("source lookup failed for {context}: {error}"))
        .len();
    let lexed = lex_loaded_source_twice(sources, source_id, source_len, context, validate_lexed);
    parse_expression_from_lexed_twice(sources, source_id, source_len, &lexed, context)
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
