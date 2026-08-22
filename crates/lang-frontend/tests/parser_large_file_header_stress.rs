//! SPEC-0170 的 4,096 项合法 / 恢复 package-import 文件头压力矩阵。

use lang_frontend::{
    lexer::{Keyword, LexemeKind, TokenKind, TriviaKind},
    parser::{Item, ParsedFile},
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{lex_parser_source_twice, parse_file_twice};

const IMPORT_COUNT: usize = 4_096;
const PACKAGE: &str = "package stress.headers";
const ROOT: &str = "val after = 1";

struct ExpectedImport {
    text: String,
    segments: Vec<String>,
    alias: Option<String>,
    wildcard: bool,
    span: (usize, usize),
}

struct ValidSource {
    text: String,
    imports: Vec<ExpectedImport>,
}

struct RecoverySource {
    text: String,
    import_spans: Vec<(usize, usize)>,
    primary_offsets: Vec<usize>,
}

fn separator(index: usize) -> &'static str {
    if index.is_multiple_of(2) {
        "\n"
    } else {
        "\r\n"
    }
}

fn valid_source() -> ValidSource {
    let mut text = format!("{PACKAGE}\n");
    let mut imports = Vec::with_capacity(IMPORT_COUNT);
    for index in 0..IMPORT_COUNT {
        let (line, segments, alias, wildcard) = match index % 4 {
            0 => {
                let segments = vec![
                    "pkg".to_owned(),
                    format!("group{index}"),
                    format!("Type{index}"),
                ];
                (
                    format!("import {}", segments.join(".")),
                    segments,
                    None,
                    false,
                )
            }
            1 => {
                let segments = vec![
                    "pkg".to_owned(),
                    format!("group{index}"),
                    format!("Type{index}"),
                ];
                let alias = format!("Alias{index}");
                (
                    format!("import {} as {alias}", segments.join(".")),
                    segments,
                    Some(alias),
                    false,
                )
            }
            2 => {
                let segments = vec!["pkg".to_owned(), format!("group{index}")];
                (
                    format!("import {}.*", segments.join(".")),
                    segments,
                    None,
                    true,
                )
            }
            _ => {
                let segments = vec![
                    "alpha".to_owned(),
                    "beta".to_owned(),
                    "gamma".to_owned(),
                    format!("delta{index}"),
                    format!("Item{index}"),
                ];
                (
                    format!("import {}", segments.join(".")),
                    segments,
                    None,
                    false,
                )
            }
        };
        let start = text.len();
        text.push_str(&line);
        let end = text.len();
        text.push_str(separator(index));
        imports.push(ExpectedImport {
            text: line,
            segments,
            alias,
            wildcard,
            span: (start, end),
        });
    }
    text.push_str(ROOT);
    ValidSource { text, imports }
}

fn recovery_source() -> RecoverySource {
    let mut text = format!("{PACKAGE}\n");
    let mut import_spans = Vec::with_capacity(IMPORT_COUNT);
    let mut primary_offsets = Vec::with_capacity(IMPORT_COUNT);
    for index in 0..IMPORT_COUNT {
        let start = text.len();
        text.push_str("import");
        let end = text.len();
        text.push_str(separator(index));
        import_spans.push((start, end));
        primary_offsets.push(text.len());
    }
    text.push_str(ROOT);
    RecoverySource {
        text,
        import_spans,
        primary_offsets,
    }
}

fn add_source(text: String, name: &str) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(name, text)
        .expect("large file header source name must be unique");
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

fn assert_lexical_header(sources: &SourceMap, source_id: SourceId, context: &str) {
    let lexed = lex_parser_source_twice(sources, source_id, context);
    assert!(lexed.diagnostics().is_empty(), "{context}");
    let import_keywords = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| {
            matches!(
                lexeme.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Import))
            )
        })
        .count();
    assert_eq!(import_keywords, IMPORT_COUNT, "{context}");

    let newlines = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Trivia(TriviaKind::Newline)))
        .map(|lexeme| {
            sources
                .slice(lexeme.span())
                .unwrap_or_else(|error| panic!("newline slice failed for {context}: {error}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(newlines.len(), IMPORT_COUNT + 1, "{context}");
    assert_eq!(
        newlines.iter().filter(|newline| **newline == "\n").count(),
        2_049,
        "{context}"
    );
    assert_eq!(
        newlines
            .iter()
            .filter(|newline| **newline == "\r\n")
            .count(),
        2_048,
        "{context}"
    );
}

fn assert_package_and_root(sources: &SourceMap, parsed: &ParsedFile, context: &str) {
    let package = parsed.package().expect("large header package");
    assert_slice(sources, package.span, PACKAGE, context);
    assert_eq!(package.segments.len(), 2, "{context}");
    assert_slice(sources, package.segments[0].span, "stress", context);
    assert_slice(sources, package.segments[1].span, "headers", context);

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
fn large_valid_and_recovered_file_headers_preserve_every_directive() {
    let valid = valid_source();
    let (sources, source_id) = add_source(valid.text, "large-valid-file-header.ko");
    assert_lexical_header(&sources, source_id, "valid header Lexer");
    let parsed = parse_file_twice(&sources, source_id, "valid header Parser");
    assert!(parsed.diagnostics().is_empty());
    assert_package_and_root(&sources, &parsed, "valid header");
    assert_eq!(parsed.imports().len(), IMPORT_COUNT);
    for (index, (actual, expected)) in parsed.imports().iter().zip(&valid.imports).enumerate() {
        let context = format!("valid import {index}");
        assert_span(actual.span, expected.span.0, expected.span.1, &context);
        assert_slice(&sources, actual.span, &expected.text, &context);
        assert_slice(&sources, actual.keyword_span, "import", &context);
        assert_eq!(actual.segments.len(), expected.segments.len(), "{context}");
        for (segment, expected_segment) in actual.segments.iter().zip(&expected.segments) {
            assert_slice(&sources, segment.span, expected_segment, &context);
        }
        assert_eq!(
            actual.wildcard_span.is_some(),
            expected.wildcard,
            "{context}"
        );
        if let Some(wildcard) = actual.wildcard_span {
            assert_slice(&sources, wildcard, "*", &context);
        }
        assert_eq!(
            actual.alias.is_some(),
            expected.alias.is_some(),
            "{context}"
        );
        if let (Some(actual_alias), Some(expected_alias)) = (actual.alias, &expected.alias) {
            assert_slice(&sources, actual_alias.as_span, "as", &context);
            assert_slice(&sources, actual_alias.name_span, expected_alias, &context);
        }
    }

    let recovery = recovery_source();
    let (sources, source_id) = add_source(recovery.text, "large-recovered-file-header.ko");
    assert_lexical_header(&sources, source_id, "recovered header Lexer");
    let parsed = parse_file_twice(&sources, source_id, "recovered header Parser");
    assert_package_and_root(&sources, &parsed, "recovered header");
    assert_eq!(parsed.imports().len(), IMPORT_COUNT);
    assert_eq!(parsed.diagnostics().len(), IMPORT_COUNT);
    for index in 0..IMPORT_COUNT {
        let context = format!("recovered import {index}");
        let import = &parsed.imports()[index];
        let (start, end) = recovery.import_spans[index];
        assert_span(import.span, start, end, &context);
        assert_span(import.keyword_span, start, end, &context);
        assert_slice(&sources, import.span, "import", &context);
        assert!(import.segments.is_empty(), "{context}");
        assert!(import.wildcard_span.is_none(), "{context}");
        assert!(import.alias.is_none(), "{context}");

        let diagnostic = &parsed.diagnostics()[index];
        assert_eq!(diagnostic.code().to_string(), "L0049", "{context}");
        assert!(diagnostic.primary_span().is_empty(), "{context}");
        assert_span(
            diagnostic.primary_span(),
            recovery.primary_offsets[index],
            recovery.primary_offsets[index],
            &context,
        );
    }
}
