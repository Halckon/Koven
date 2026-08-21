//! Lexer matrix 共享的双运行公开产物不变量。

use lang_frontend::{
    lexer::{LexedFile, lex},
    source::{SourceId, SourceMap},
};

pub(crate) fn lex_source_twice(
    source_name: &str,
    source: &str,
    context: &str,
    validate: fn(SourceId, usize, &LexedFile),
) -> (SourceMap, SourceId, LexedFile) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(source_name, source)
        .unwrap_or_else(|error| panic!("source setup failed for {context}: {error}"));
    let first = lex(&sources, source_id)
        .unwrap_or_else(|error| panic!("first lex failed for {context}: {error}"));
    let repeated = lex(&sources, source_id)
        .unwrap_or_else(|error| panic!("repeated lex failed for {context}: {error}"));
    validate(source_id, source.len(), &first);
    validate(source_id, source.len(), &repeated);
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic lex for {context}"
    );
    (sources, source_id, first)
}
