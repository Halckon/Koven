//! Lexer / 完整文件 Parser 矩阵共用的公开不变量断言。

use lang_frontend::{
    lexer::LexedFile,
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
};

#[path = "frontend_output_assertions.rs"]
mod frontend_output_assertions;

pub(crate) use frontend_output_assertions::validate_lexed;
use frontend_output_assertions::{validate_ast, validate_diagnostics};

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
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic parse for {context}"
    );

    validate_diagnostics(source_id, source_len, first.diagnostics());
    validate_ast(source_id, source_len, first.ast());
    first
}
