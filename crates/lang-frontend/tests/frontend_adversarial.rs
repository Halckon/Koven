//! SPEC-0068 / SPEC-0102 的 Lexer / 完整文件 Parser 确定性对抗组合矩阵。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexedFile, lex},
    source::{SourceId, SourceMap},
};

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;

use frontend_matrix_assertions::{parse_file_twice, validate_lexed};

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

fn diagnostic_fingerprints(diagnostics: &[Diagnostic]) -> Vec<DiagnosticFingerprint> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code().to_string(),
                diagnostic.message().to_owned(),
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end(),
            )
        })
        .collect()
}

fn lex_twice(
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    context: &str,
) -> LexedFile {
    let first = lex(sources, source_id)
        .unwrap_or_else(|error| panic!("first lex failed for {context}: {error}"));
    let repeated = lex(sources, source_id)
        .unwrap_or_else(|error| panic!("repeated lex failed for {context}: {error}"));
    validate_lexed(source_id, source_len, &first);
    validate_lexed(source_id, source_len, &repeated);
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic lex for {context}"
    );
    first
}

#[test]
fn short_source_matrix_preserves_coverage_and_repeatable_parse_structure() {
    assert_eq!(PREFIXES.len(), 18);
    assert_eq!(SUFFIXES.len(), 18);
    assert_eq!(PREFIXES.len() * SUFFIXES.len(), 324);
    for prefix in PREFIXES {
        for suffix in SUFFIXES {
            let text = format!("{prefix}{suffix}");
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("adversarial.ko", text.clone())
                .expect("unique matrix source");
            let context = format!("adversarial case {text:?}");
            let lexed = lex_twice(&sources, source_id, text.len(), &context);
            parse_file_twice(&sources, source_id, text.len(), &lexed, &context);
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
    let lexed = lex_twice(&sources, source_id, text.len(), "terminal char regression");
    assert_eq!(
        diagnostic_fingerprints(lexed.diagnostics()),
        [(
            "L0007".to_owned(),
            "invalid character literal".to_owned(),
            11,
            12
        )]
    );

    let parsed = parse_file_twice(
        &sources,
        source_id,
        text.len(),
        &lexed,
        "terminal char regression",
    );
    assert_eq!(
        diagnostic_fingerprints(parsed.diagnostics()),
        [(
            "L0007".to_owned(),
            "invalid character literal".to_owned(),
            11,
            12
        )]
    );
}
