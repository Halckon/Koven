//! SPEC-0069 的三个独立 Parser 入口确定性对抗组合矩阵。

use lang_frontend::{
    lexer::{LexedFile, lex},
    parser::{parse_block, parse_declaration, parse_expression},
    source::{SourceId, SourceMap},
};

const PREFIXES: &[&str] = &[
    "",
    "x",
    "val x = ",
    "fun f(",
    "fun f(): T ",
    "{",
    "{ val x = ",
    "{ x -> ",
    "call(",
    "value[",
    "if (",
    "when (x) {",
    "\"${",
    "'",
    "/*",
    "β",
];

const SUFFIXES: &[&str] = &[
    "", "0", "x", ")", "]", "}", ">", ",", " + y", " ?: y", "?.member", "\n", ";", "\"", "'", "\0",
];

type ParseEntry = fn(&SourceMap, &LexedFile) -> Result<String, String>;

fn expression_fingerprint(sources: &SourceMap, lexed: &LexedFile) -> Result<String, String> {
    parse_expression(sources, lexed)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

fn declaration_fingerprint(sources: &SourceMap, lexed: &LexedFile) -> Result<String, String> {
    parse_declaration(sources, lexed)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

fn block_fingerprint(sources: &SourceMap, lexed: &LexedFile) -> Result<String, String> {
    parse_block(sources, lexed)
        .map(|parsed| format!("{parsed:?}"))
        .map_err(|error| error.to_string())
}

fn exercise_matrix(entry_name: &str, parse: ParseEntry) -> usize {
    let mut executed = 0;
    for prefix in PREFIXES {
        for suffix in SUFFIXES {
            let text = format!("{prefix}{suffix}");
            let mut sources = SourceMap::new();
            let source_id: SourceId = sources
                .add_source("entry-adversarial.ko", text.clone())
                .expect("unique matrix source");
            let lexed = lex(&sources, source_id).expect("matrix lexing must not fail internally");
            let first = parse(&sources, &lexed).unwrap_or_else(|error| {
                panic!("{entry_name} case {text:?} failed internally: {error}")
            });
            let repeated = parse(&sources, &lexed).unwrap_or_else(|error| {
                panic!("repeated {entry_name} case {text:?} failed internally: {error}")
            });
            assert_eq!(
                first, repeated,
                "non-deterministic {entry_name} case: {text:?}"
            );
            executed += 1;
        }
    }
    executed
}

#[test]
fn independent_parser_entries_are_total_and_deterministic_over_the_matrix() {
    assert_eq!(PREFIXES.len(), 16);
    assert_eq!(SUFFIXES.len(), 16);
    let entries = [
        ("expression", expression_fingerprint as ParseEntry),
        ("declaration", declaration_fingerprint as ParseEntry),
        ("block", block_fingerprint as ParseEntry),
    ];

    let executed = entries
        .into_iter()
        .map(|(name, parse)| exercise_matrix(name, parse))
        .sum::<usize>();
    assert_eq!(executed, 16 * 16 * 3);
}
