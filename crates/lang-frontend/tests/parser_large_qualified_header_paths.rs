//! SPEC-0171 的 4,096-segment package/import 路径与末尾恢复矩阵。

use lang_frontend::{
    lexer::{LexemeKind, Symbol, TokenKind},
    parser::{Item, ParsedFile, QualifiedNameSegment},
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{lex_parser_source_twice, parse_file_twice};

const SEGMENT_COUNT: usize = 4_096;
const ROOT: &str = "val after = 1";

struct ValidSource {
    text: String,
    package_span: (usize, usize),
    exact_span: (usize, usize),
    exact_alias_as: (usize, usize),
    exact_alias_name: (usize, usize),
    wildcard_span: (usize, usize),
    wildcard_marker: (usize, usize),
}

struct RecoverySource {
    text: String,
    package_span: (usize, usize),
    package_dot: (usize, usize),
    trailing_import_span: (usize, usize),
    trailing_import_dot: (usize, usize),
    alias_import_start: usize,
    alias_as: (usize, usize),
    root_start: usize,
}

fn push_path(text: &mut String, prefix: char) {
    for index in 0..SEGMENT_COUNT {
        if index > 0 {
            text.push('.');
        }
        text.push(prefix);
        text.push_str(&index.to_string());
    }
}

fn valid_source() -> ValidSource {
    let mut text = "package ".to_owned();
    push_path(&mut text, 'p');
    let package_span = (0, text.len());
    text.push('\n');

    let exact_start = text.len();
    text.push_str("import ");
    push_path(&mut text, 'q');
    text.push(' ');
    let alias_as_start = text.len();
    text.push_str("as");
    let alias_as_end = text.len();
    text.push(' ');
    let alias_name_start = text.len();
    text.push_str("Alias");
    let alias_name_end = text.len();
    let exact_span = (exact_start, text.len());
    text.push_str("\r\n");

    let wildcard_start = text.len();
    text.push_str("import ");
    push_path(&mut text, 'w');
    text.push('.');
    let wildcard_start_marker = text.len();
    text.push('*');
    let wildcard_end_marker = text.len();
    let wildcard_span = (wildcard_start, text.len());
    text.push('\n');
    text.push_str(ROOT);

    ValidSource {
        text,
        package_span,
        exact_span,
        exact_alias_as: (alias_as_start, alias_as_end),
        exact_alias_name: (alias_name_start, alias_name_end),
        wildcard_span,
        wildcard_marker: (wildcard_start_marker, wildcard_end_marker),
    }
}

fn recovery_source() -> RecoverySource {
    let mut text = "package ".to_owned();
    push_path(&mut text, 'p');
    let package_dot_start = text.len();
    text.push('.');
    let package_dot_end = text.len();
    let package_span = (0, text.len());
    text.push('\n');

    let trailing_import_start = text.len();
    text.push_str("import ");
    push_path(&mut text, 'q');
    let trailing_import_dot_start = text.len();
    text.push('.');
    let trailing_import_dot_end = text.len();
    let trailing_import_span = (trailing_import_start, text.len());
    text.push_str("\r\n");

    let alias_import_start = text.len();
    text.push_str("import ");
    push_path(&mut text, 'a');
    text.push(' ');
    let alias_as_start = text.len();
    text.push_str("as");
    let alias_as_end = text.len();
    text.push('\n');
    let root_start = text.len();
    text.push_str(ROOT);

    RecoverySource {
        text,
        package_span,
        package_dot: (package_dot_start, package_dot_end),
        trailing_import_span,
        trailing_import_dot: (trailing_import_dot_start, trailing_import_dot_end),
        alias_import_start,
        alias_as: (alias_as_start, alias_as_end),
        root_start,
    }
}

fn add_source(text: String, name: &str) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(name, text)
        .expect("large qualified header source name must be unique");
    (sources, source_id)
}

fn assert_span(span: Span, start: usize, end: usize, context: &str) {
    assert_eq!((span.start(), span.end()), (start, end), "{context}");
}

fn assert_slice(sources: &SourceMap, span: Span, expected: &str, context: &str) {
    assert_eq!(
        sources
            .slice(span)
            .unwrap_or_else(|error| panic!("source slice failed for {context}: {error}")),
        expected,
        "{context}"
    );
}

fn assert_lexical_shape(
    sources: &SourceMap,
    source_id: SourceId,
    expected_identifiers: usize,
    expected_dots: usize,
    context: &str,
) {
    let lexed = lex_parser_source_twice(sources, source_id, context);
    assert!(lexed.diagnostics().is_empty(), "{context}");
    assert_eq!(
        lexed
            .lexemes()
            .iter()
            .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)))
            .count(),
        expected_identifiers,
        "{context}"
    );
    assert_eq!(
        lexed
            .lexemes()
            .iter()
            .filter(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Dot))
                )
            })
            .count(),
        expected_dots,
        "{context}"
    );
}

fn assert_segments(
    sources: &SourceMap,
    segments: &[QualifiedNameSegment],
    prefix: char,
    context: &str,
) {
    assert_eq!(segments.len(), SEGMENT_COUNT, "{context}");
    for (index, segment) in segments.iter().enumerate() {
        assert_slice(sources, segment.span, &format!("{prefix}{index}"), context);
    }
}

fn assert_root(sources: &SourceMap, parsed: &ParsedFile, context: &str) {
    assert_eq!(parsed.roots().len(), 1, "{context}");
    let root = parsed
        .ast()
        .items()
        .get(parsed.roots()[0])
        .unwrap_or_else(|error| panic!("root lookup failed for {context}: {error}"));
    assert!(matches!(root.payload(), Item::Variable { .. }), "{context}");
    assert_slice(sources, root.span(), ROOT, context);
}

#[test]
fn large_qualified_header_paths_preserve_all_segments_and_terminal_recovery() {
    let valid = valid_source();
    let (sources, source_id) = add_source(valid.text, "large-valid-qualified-header.ko");
    assert_lexical_shape(&sources, source_id, 12_290, 12_286, "valid header Lexer");
    let parsed = parse_file_twice(&sources, source_id, "valid header Parser");
    assert!(parsed.diagnostics().is_empty());
    let package = parsed.package().expect("valid package");
    assert_span(
        package.span,
        valid.package_span.0,
        valid.package_span.1,
        "valid package",
    );
    assert_segments(&sources, &package.segments, 'p', "valid package");
    assert_eq!(parsed.imports().len(), 2);
    let exact = &parsed.imports()[0];
    assert_span(
        exact.span,
        valid.exact_span.0,
        valid.exact_span.1,
        "exact import",
    );
    assert_segments(&sources, &exact.segments, 'q', "exact import");
    assert!(exact.wildcard_span.is_none());
    let alias = exact.alias.expect("valid alias");
    assert_span(
        alias.as_span,
        valid.exact_alias_as.0,
        valid.exact_alias_as.1,
        "valid alias as",
    );
    assert_slice(&sources, alias.as_span, "as", "valid alias as");
    assert_span(
        alias.name_span,
        valid.exact_alias_name.0,
        valid.exact_alias_name.1,
        "valid alias name",
    );
    assert_slice(&sources, alias.name_span, "Alias", "valid alias");
    let wildcard = &parsed.imports()[1];
    assert_span(
        wildcard.span,
        valid.wildcard_span.0,
        valid.wildcard_span.1,
        "wildcard import",
    );
    assert_segments(&sources, &wildcard.segments, 'w', "wildcard import");
    let marker = wildcard.wildcard_span.expect("valid wildcard");
    assert_span(
        marker,
        valid.wildcard_marker.0,
        valid.wildcard_marker.1,
        "wildcard marker",
    );
    assert_slice(&sources, marker, "*", "wildcard marker");
    assert!(wildcard.alias.is_none());
    assert_root(&sources, &parsed, "valid header");

    let recovery = recovery_source();
    let (sources, source_id) = add_source(recovery.text, "large-recovered-qualified-header.ko");
    assert_lexical_shape(
        &sources,
        source_id,
        12_289,
        12_287,
        "recovered header Lexer",
    );
    let parsed = parse_file_twice(&sources, source_id, "recovered header Parser");
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0048", "L0049", "L0050"]
    );
    let package = parsed.package().expect("recovered package");
    assert_span(
        package.span,
        recovery.package_span.0,
        recovery.package_span.1,
        "recovered package",
    );
    assert_segments(&sources, &package.segments, 'p', "recovered package");
    assert_slice(
        &sources,
        sources
            .span(
                package.span.source_id(),
                recovery.package_dot.0,
                recovery.package_dot.1,
            )
            .expect("package dot Span"),
        ".",
        "package trailing dot",
    );

    assert_eq!(parsed.imports().len(), 2);
    let trailing = &parsed.imports()[0];
    assert_span(
        trailing.span,
        recovery.trailing_import_span.0,
        recovery.trailing_import_span.1,
        "trailing-dot import",
    );
    assert_segments(&sources, &trailing.segments, 'q', "trailing-dot import");
    assert_slice(
        &sources,
        sources
            .span(
                trailing.span.source_id(),
                recovery.trailing_import_dot.0,
                recovery.trailing_import_dot.1,
            )
            .expect("import dot Span"),
        ".",
        "import trailing dot",
    );

    let missing_alias = &parsed.imports()[1];
    assert_eq!(missing_alias.span.start(), recovery.alias_import_start);
    assert_eq!(missing_alias.span.end(), recovery.root_start);
    assert_segments(
        &sources,
        &missing_alias.segments,
        'a',
        "missing alias import",
    );
    let alias = missing_alias.alias.expect("recovered alias marker");
    assert_span(
        alias.as_span,
        recovery.alias_as.0,
        recovery.alias_as.1,
        "recovered alias as",
    );
    assert_slice(&sources, alias.as_span, "as", "recovered alias as");
    assert_span(
        alias.name_span,
        recovery.root_start,
        recovery.root_start,
        "recovered alias name",
    );
    for (diagnostic, expected) in parsed.diagnostics().iter().zip([
        recovery.trailing_import_span.0,
        recovery.alias_import_start,
        recovery.root_start,
    ]) {
        assert_span(
            diagnostic.primary_span(),
            expected,
            expected,
            "recovery diagnostic",
        );
    }
    assert_root(&sources, &parsed, "recovered header");
}
