//! SPEC-0068 的 Lexer / 完整文件 Parser 确定性对抗组合矩阵。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexedFile, LexemeKind, lex},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap, Span},
};

const PREFIXES: &[&str] = &[
    "",
    "val x = ",
    "fun f(",
    "fun f(): T { ",
    "class C(",
    "class C { ",
    "if (",
    "when (x) { ",
    "for (x in ",
    "call(",
    "value[",
    "{ x -> ",
    "\"${",
    "\"text",
    "'",
    "/* comment",
    "// comment",
    "β\0",
];

const SUFFIXES: &[&str] = &[
    "",
    "x",
    "0",
    "\nval after = 1",
    "; fun after() {}",
    ")",
    "]",
    "}",
    ">",
    ",",
    " +",
    " ?: y",
    "?.member",
    "\"",
    "'",
    "*/",
    "\n",
    "😀",
];

type DiagnosticFingerprint = (String, String, usize, usize);
type SpanFingerprint = (usize, usize);

#[derive(Debug, PartialEq, Eq)]
struct ParseFingerprint {
    diagnostics: Vec<DiagnosticFingerprint>,
    package: Option<SpanFingerprint>,
    imports: Vec<SpanFingerprint>,
    roots: Vec<(usize, SpanFingerprint)>,
    items: Vec<SpanFingerprint>,
    statements: Vec<SpanFingerprint>,
    expressions: Vec<SpanFingerprint>,
    type_refs: Vec<SpanFingerprint>,
}

fn span_fingerprint(span: Span) -> SpanFingerprint {
    (span.start(), span.end())
}

fn diagnostic_fingerprints(
    diagnostics: &[Diagnostic],
    source_id: SourceId,
    source_len: usize,
) -> Vec<DiagnosticFingerprint> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            assert_span(source_id, source_len, diagnostic.primary_span());
            (
                diagnostic.code().to_string(),
                diagnostic.message().to_owned(),
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end(),
            )
        })
        .collect()
}

fn assert_span(source_id: SourceId, source_len: usize, span: Span) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
}

fn assert_lexeme_coverage(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
    let mut offset = 0;
    let mut eof_count = 0;
    for lexeme in lexed.lexemes() {
        let span = lexeme.span();
        assert_span(source_id, source_len, span);
        assert_eq!(span.start(), offset);
        if lexeme.kind() == LexemeKind::Eof {
            eof_count += 1;
            assert_eq!(span_fingerprint(span), (source_len, source_len));
        } else {
            assert!(span.end() > span.start());
            offset = span.end();
        }
    }
    assert_eq!(offset, source_len);
    assert_eq!(eof_count, 1);
}

fn parse_fingerprint(
    parsed: &ParsedFile,
    source_id: SourceId,
    source_len: usize,
) -> ParseFingerprint {
    let ast = parsed.ast();
    let collect_spans = |spans: Vec<Span>| {
        spans
            .into_iter()
            .map(|span| {
                assert_span(source_id, source_len, span);
                span_fingerprint(span)
            })
            .collect::<Vec<_>>()
    };
    ParseFingerprint {
        diagnostics: diagnostic_fingerprints(parsed.diagnostics(), source_id, source_len),
        package: parsed.package().map(|package| {
            assert_span(source_id, source_len, package.span);
            span_fingerprint(package.span)
        }),
        imports: collect_spans(parsed.imports().iter().map(|import| import.span).collect()),
        roots: parsed
            .roots()
            .iter()
            .map(|root| {
                let span = ast.items().get(*root).expect("valid root ID").span();
                assert_span(source_id, source_len, span);
                (root.index(), span_fingerprint(span))
            })
            .collect(),
        items: collect_spans(ast.items().iter().map(|(_, node)| node.span()).collect()),
        statements: collect_spans(
            ast.statements()
                .iter()
                .map(|(_, node)| node.span())
                .collect(),
        ),
        expressions: collect_spans(
            ast.expressions()
                .iter()
                .map(|(_, node)| node.span())
                .collect(),
        ),
        type_refs: collect_spans(
            ast.type_refs()
                .iter()
                .map(|(_, node)| node.span())
                .collect(),
        ),
    }
}

#[test]
fn short_source_matrix_preserves_coverage_and_repeatable_parse_structure() {
    assert_eq!(PREFIXES.len() * SUFFIXES.len(), 324);
    for prefix in PREFIXES {
        for suffix in SUFFIXES {
            let text = format!("{prefix}{suffix}");
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("adversarial.ko", text.clone())
                .expect("unique matrix source");
            let lexed = lex(&sources, source_id).expect("matrix lexing must not fail internally");
            assert_lexeme_coverage(source_id, text.len(), &lexed);
            diagnostic_fingerprints(lexed.diagnostics(), source_id, text.len());

            let first = parse_file(&sources, &lexed).unwrap_or_else(|error| {
                panic!(
                    "matrix case {text:?} failed internally: {error:?}; lexemes={:?}; diagnostics={:?}",
                    lexed.lexemes(),
                    lexed.diagnostics()
                )
            });
            let repeated = parse_file(&sources, &lexed).unwrap_or_else(|error| {
                panic!("repeated matrix case {text:?} failed internally: {error:?}")
            });
            assert_eq!(
                parse_fingerprint(&first, source_id, text.len()),
                parse_fingerprint(&repeated, source_id, text.len()),
                "non-deterministic case: {text:?}"
            );
        }
    }
}

#[test]
fn terminal_char_root_closes_suppressed_string_and_interpolation_owners() {
    let text = "val x = \"${'";
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("terminal-char.ko", text)
        .expect("unique regression source");
    let lexed = lex(&sources, source_id).expect("regression source must lex");
    assert_eq!(
        diagnostic_fingerprints(lexed.diagnostics(), source_id, text.len()),
        [(
            "L0007".to_owned(),
            "invalid character literal".to_owned(),
            11,
            12
        )]
    );

    let parsed =
        parse_file(&sources, &lexed).expect("terminal lexical root must remain recoverable");
    assert_eq!(
        diagnostic_fingerprints(parsed.diagnostics(), source_id, text.len()),
        [(
            "L0007".to_owned(),
            "invalid character literal".to_owned(),
            11,
            12
        )]
    );
}
