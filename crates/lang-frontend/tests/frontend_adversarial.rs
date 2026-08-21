//! SPEC-0068 / SPEC-0102 / SPEC-0103 的 Lexer / 完整文件 Parser 确定性对抗组合矩阵。

use lang_frontend::diagnostic::Diagnostic;

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use frontend_matrix_assertions::{parse_file_twice, validate_lexed};
use lexer_matrix_assertions::lex_source_twice;

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

#[test]
fn short_source_matrix_preserves_coverage_and_repeatable_parse_structure() {
    assert_eq!(PREFIXES.len(), 18);
    assert_eq!(SUFFIXES.len(), 18);
    assert_eq!(PREFIXES.len() * SUFFIXES.len(), 324);
    for prefix in PREFIXES {
        for suffix in SUFFIXES {
            let text = format!("{prefix}{suffix}");
            let context = format!("adversarial case {text:?}");
            let (sources, source_id, lexed) =
                lex_source_twice("adversarial.ko", &text, &context, validate_lexed);
            parse_file_twice(&sources, source_id, text.len(), &lexed, &context);
        }
    }
}

#[test]
fn terminal_char_root_closes_suppressed_string_and_interpolation_owners() {
    let text = "val x = \"${'";
    let (sources, source_id, lexed) = lex_source_twice(
        "terminal-char.ko",
        text,
        "terminal char regression",
        validate_lexed,
    );
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
