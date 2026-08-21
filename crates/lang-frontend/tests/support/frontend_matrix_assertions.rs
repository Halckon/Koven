//! SPEC-0099 / SPEC-0111 的 Lexer / 完整文件 Parser 矩阵共享公开产物不变量。

use lang_frontend::{
    lexer::LexedFile,
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
};

#[path = "file_output_assertions.rs"]
mod file_output_assertions;
#[path = "frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use file_output_assertions::validate_file_output;
use frontend_output_assertions::validate_lexed;

pub(crate) fn lex_source_twice(
    source_name: &str,
    source: &str,
    context: &str,
) -> (SourceMap, SourceId, LexedFile) {
    lexer_matrix_assertions::lex_source_twice(source_name, source, context, validate_lexed)
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
