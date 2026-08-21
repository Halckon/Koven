//! SPEC-0099 的 Lexer / 完整文件 Parser 矩阵共享公开产物不变量。

use lang_frontend::{
    lexer::LexedFile,
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
};

#[path = "frontend_output_assertions.rs"]
mod frontend_output_assertions;

pub(crate) use frontend_output_assertions::validate_lexed;
use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_span};

fn validate_file_output(
    source_id: SourceId,
    source_len: usize,
    parsed: &ParsedFile,
    context: &str,
) {
    assert_eq!(parsed.source_id(), source_id);
    validate_ast(source_id, source_len, parsed.ast());
    validate_diagnostics(source_id, source_len, parsed.diagnostics());
    for root in parsed.roots() {
        parsed
            .ast()
            .items()
            .get(*root)
            .unwrap_or_else(|error| panic!("invalid file root for {context}: {error}"));
    }
    if let Some(package) = parsed.package() {
        validate_span(source_id, source_len, package.span);
        validate_span(source_id, source_len, package.keyword_span);
        for segment in &package.segments {
            validate_span(source_id, source_len, segment.span);
        }
    }
    for import in parsed.imports() {
        validate_span(source_id, source_len, import.span);
        validate_span(source_id, source_len, import.keyword_span);
        for segment in &import.segments {
            validate_span(source_id, source_len, segment.span);
        }
        if let Some(span) = import.wildcard_span {
            validate_span(source_id, source_len, span);
        }
        if let Some(alias) = import.alias {
            validate_span(source_id, source_len, alias.as_span);
            validate_span(source_id, source_len, alias.name_span);
        }
    }
}

pub(crate) fn parse_file_twice(
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
    context: &str,
) -> ParsedFile {
    let first = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
    let repeated = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("repeated parse failed for {context}: {error}"));
    validate_file_output(source_id, source_len, &first, context);
    validate_file_output(source_id, source_len, &repeated, context);
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic parse for {context}"
    );

    first
}
