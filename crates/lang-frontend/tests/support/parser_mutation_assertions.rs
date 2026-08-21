//! Parser source-mutation matrices shared assertions.

use lang_frontend::{parser::ParsedFile, source::SourceMap};

pub(crate) fn assert_last_root_source(
    sources: &SourceMap,
    parsed: &ParsedFile,
    expected: &str,
    context: &str,
) {
    let last_root = parsed
        .roots()
        .last()
        .unwrap_or_else(|| panic!("missing expected last root for {context}"));
    let span = parsed
        .ast()
        .items()
        .get(*last_root)
        .unwrap_or_else(|error| panic!("invalid last root for {context}: {error}"))
        .span();
    assert_eq!(
        sources
            .slice(span)
            .unwrap_or_else(|error| panic!("invalid last root span for {context}: {error}")),
        expected,
        "last root source mismatch for {context}; parsed={parsed:?}"
    );
}
