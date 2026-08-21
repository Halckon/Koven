//! SPEC-0099 / SPEC-0124 complete-file Parser shared source-local output assertions.

use lang_frontend::{parser::ParsedFile, source::SourceId};

use super::frontend_output_assertions::{validate_ast, validate_diagnostics, validate_span};

pub(crate) fn validate_file_output(
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
