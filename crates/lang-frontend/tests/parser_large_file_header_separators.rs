//! SPEC-0172 的 4,096-import 文件头混合分隔与 L0053 恢复矩阵。

use lang_frontend::{
    lexer::{Keyword, LexemeKind, Symbol, TokenKind, TriviaKind},
    parser::{Item, ParsedFile},
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{lex_parser_source_twice, parse_file_twice};

const IMPORT_COUNT: usize = 4_096;
const PACKAGE: &str = "package stress.separators";
const ROOT: &str = "val after = 1";

struct HeaderSource {
    text: String,
    import_spans: Vec<(usize, usize)>,
    root_start: usize,
}

struct RecoverySource {
    header: HeaderSource,
    diagnostic_starts: Vec<usize>,
}

fn mixed_separator(index: usize) -> &'static str {
    match index % 4 {
        0 => "\n",
        1 => "\r\n",
        2 => ";",
        _ => " /* header\nbreak */ ",
    }
}

fn push_import(text: &mut String, index: usize) -> (usize, usize) {
    let start = text.len();
    text.push_str("import pkg.Item");
    text.push_str(&index.to_string());
    (start, text.len())
}

fn valid_source() -> HeaderSource {
    let mut text = format!("{PACKAGE}\n");
    let mut import_spans = Vec::with_capacity(IMPORT_COUNT);
    for index in 0..IMPORT_COUNT {
        import_spans.push(push_import(&mut text, index));
        text.push_str(mixed_separator(index));
    }
    let root_start = text.len();
    text.push_str(ROOT);
    HeaderSource {
        text,
        import_spans,
        root_start,
    }
}

fn recovery_source() -> RecoverySource {
    let mut text = PACKAGE.to_owned();
    text.push(' ');
    let mut import_spans = Vec::with_capacity(IMPORT_COUNT);
    let mut diagnostic_starts = Vec::with_capacity(IMPORT_COUNT + 1);
    for index in 0..IMPORT_COUNT {
        let import_span = push_import(&mut text, index);
        diagnostic_starts.push(import_span.0);
        import_spans.push(import_span);
        text.push(' ');
    }
    let root_start = text.len();
    diagnostic_starts.push(root_start);
    text.push_str(ROOT);
    RecoverySource {
        header: HeaderSource {
            text,
            import_spans,
            root_start,
        },
        diagnostic_starts,
    }
}

fn add_source(text: String, name: &str) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(name, text)
        .expect("large file header separator source name must be unique");
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

fn count_lexemes(
    lexemes: &[lang_frontend::lexer::Lexeme],
    predicate: impl Fn(LexemeKind) -> bool,
) -> usize {
    lexemes
        .iter()
        .filter(|lexeme| predicate(lexeme.kind()))
        .count()
}

fn assert_lexical_shape(sources: &SourceMap, source_id: SourceId, mixed: bool, context: &str) {
    let lexed = lex_parser_source_twice(sources, source_id, context);
    assert!(lexed.diagnostics().is_empty(), "{context}");
    assert_eq!(
        count_lexemes(lexed.lexemes(), |kind| matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Import))
        )),
        IMPORT_COUNT,
        "{context}"
    );
    assert_eq!(
        count_lexemes(lexed.lexemes(), |kind| matches!(
            kind,
            LexemeKind::Token(TokenKind::Identifier)
        )),
        8_195,
        "{context}"
    );
    assert_eq!(
        count_lexemes(lexed.lexemes(), |kind| matches!(
            kind,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot))
        )),
        4_097,
        "{context}"
    );

    let expected_structural_count = if mixed { 1_024 } else { 0 };
    assert_eq!(
        count_lexemes(lexed.lexemes(), |kind| matches!(
            kind,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
        )),
        expected_structural_count,
        "{context}"
    );
    assert_eq!(
        count_lexemes(lexed.lexemes(), |kind| matches!(
            kind,
            LexemeKind::Trivia(TriviaKind::BlockComment)
        )),
        expected_structural_count,
        "{context}"
    );

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
    if mixed {
        assert_eq!(newlines.len(), 2_049, "{context}");
        assert_eq!(
            newlines.iter().filter(|newline| **newline == "\n").count(),
            1_025,
            "{context}"
        );
        assert_eq!(
            newlines
                .iter()
                .filter(|newline| **newline == "\r\n")
                .count(),
            1_024,
            "{context}"
        );
    } else {
        assert!(newlines.is_empty(), "{context}");
    }
}

fn assert_header(
    sources: &SourceMap,
    parsed: &ParsedFile,
    import_spans: &[(usize, usize)],
    root_start: usize,
    context: &str,
) {
    let package = parsed.package().expect("large separator package");
    assert_slice(sources, package.span, PACKAGE, context);
    assert_slice(sources, package.keyword_span, "package", context);
    assert_eq!(package.segments.len(), 2, "{context}");
    assert_slice(sources, package.segments[0].span, "stress", context);
    assert_slice(sources, package.segments[1].span, "separators", context);

    assert_eq!(parsed.imports().len(), IMPORT_COUNT, "{context}");
    for (index, (import, expected_span)) in parsed.imports().iter().zip(import_spans).enumerate() {
        let item_context = format!("{context} import {index}");
        assert_span(import.span, expected_span.0, expected_span.1, &item_context);
        assert_slice(sources, import.keyword_span, "import", &item_context);
        assert_eq!(import.segments.len(), 2, "{item_context}");
        assert_slice(sources, import.segments[0].span, "pkg", &item_context);
        assert_slice(
            sources,
            import.segments[1].span,
            &format!("Item{index}"),
            &item_context,
        );
        assert!(import.wildcard_span.is_none(), "{item_context}");
        assert!(import.alias.is_none(), "{item_context}");
    }

    assert_eq!(parsed.roots().len(), 1, "{context}");
    let root = parsed
        .ast()
        .items()
        .get(parsed.roots()[0])
        .unwrap_or_else(|error| panic!("root lookup failed for {context}: {error}"));
    assert!(matches!(root.payload(), Item::Variable { .. }), "{context}");
    assert_span(root.span(), root_start, root_start + ROOT.len(), context);
    assert_slice(sources, root.span(), ROOT, context);
}

#[test]
fn large_file_headers_preserve_mixed_separators_and_every_missing_boundary() {
    let HeaderSource {
        text,
        import_spans,
        root_start,
    } = valid_source();
    let (sources, source_id) = add_source(text, "large-mixed-header-separators.ko");
    assert_lexical_shape(&sources, source_id, true, "mixed header Lexer");
    let parsed = parse_file_twice(&sources, source_id, "mixed header Parser");
    assert!(parsed.diagnostics().is_empty());
    assert_header(&sources, &parsed, &import_spans, root_start, "mixed header");

    let RecoverySource {
        header:
            HeaderSource {
                text,
                import_spans,
                root_start,
            },
        diagnostic_starts,
    } = recovery_source();
    let (sources, source_id) = add_source(text, "large-missing-header-separators.ko");
    assert_lexical_shape(&sources, source_id, false, "missing separator Lexer");
    let parsed = parse_file_twice(&sources, source_id, "missing separator Parser");
    assert_header(
        &sources,
        &parsed,
        &import_spans,
        root_start,
        "missing separator header",
    );
    assert_eq!(parsed.diagnostics().len(), IMPORT_COUNT + 1);
    for (index, (diagnostic, expected_start)) in parsed
        .diagnostics()
        .iter()
        .zip(&diagnostic_starts)
        .enumerate()
    {
        let context = format!("missing separator diagnostic {index}");
        assert_eq!(diagnostic.code().to_string(), "L0053", "{context}");
        let expected_len = if index < IMPORT_COUNT { 6 } else { 3 };
        assert_span(
            diagnostic.primary_span(),
            *expected_start,
            expected_start + expected_len,
            &context,
        );
        assert_slice(
            &sources,
            diagnostic.primary_span(),
            if index < IMPORT_COUNT {
                "import"
            } else {
                "val"
            },
            &context,
        );
    }
}
