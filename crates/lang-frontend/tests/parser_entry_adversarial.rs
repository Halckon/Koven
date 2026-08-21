//! SPEC-0069 / SPEC-0093 / SPEC-0105 的独立 Parser 入口对抗组合产物不变量。

use lang_frontend::{
    parser::{parse_block, parse_declaration, parse_expression},
    source::{SourceId, SourceMap},
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use lexer_matrix_assertions::lex_source_twice;

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

#[derive(Clone, Copy)]
enum EntryKind {
    Expression,
    Declaration,
    Block,
}

fn parse_twice(
    entry_name: &str,
    kind: EntryKind,
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &lang_frontend::lexer::LexedFile,
    text: &str,
) {
    macro_rules! parse_entry {
        ($parse:ident, $table:ident) => {{
            let first = $parse(sources, lexed).unwrap_or_else(|error| {
                panic!("{entry_name} case {text:?} failed internally: {error}")
            });
            let repeated = $parse(sources, lexed).unwrap_or_else(|error| {
                panic!("repeated {entry_name} case {text:?} failed internally: {error}")
            });
            for parsed in [&first, &repeated] {
                assert_eq!(parsed.source_id(), source_id);
                validate_ast(source_id, source_len, parsed.ast());
                validate_diagnostics(source_id, source_len, parsed.diagnostics());
                parsed
                    .ast()
                    .$table()
                    .get(parsed.root())
                    .unwrap_or_else(|error| {
                        panic!("invalid {entry_name} root for {text:?}: {error}")
                    });
            }
            assert_eq!(
                format!("{first:?}"),
                format!("{repeated:?}"),
                "non-deterministic {entry_name} case: {text:?}"
            );
        }};
    }

    match kind {
        EntryKind::Expression => parse_entry!(parse_expression, expressions),
        EntryKind::Declaration => parse_entry!(parse_declaration, items),
        EntryKind::Block => parse_entry!(parse_block, statements),
    }
}

fn exercise_matrix(entry_name: &str, kind: EntryKind) -> usize {
    let mut executed = 0;
    for prefix in PREFIXES {
        for suffix in SUFFIXES {
            let text = format!("{prefix}{suffix}");
            let context = format!("{entry_name} case {text:?}");
            let (sources, source_id, lexed) =
                lex_source_twice("entry-adversarial.ko", &text, &context, validate_lexed);
            parse_twice(
                entry_name,
                kind,
                &sources,
                source_id,
                text.len(),
                &lexed,
                &text,
            );
            executed += 1;
        }
    }
    executed
}

#[test]
fn independent_parser_entries_preserve_output_invariants_over_the_matrix() {
    assert_eq!(PREFIXES.len(), 16);
    assert_eq!(SUFFIXES.len(), 16);
    let entries = [
        ("expression", EntryKind::Expression),
        ("declaration", EntryKind::Declaration),
        ("block", EntryKind::Block),
    ];

    let executed = entries
        .into_iter()
        .map(|(name, kind)| exercise_matrix(name, kind))
        .sum::<usize>();
    assert_eq!(executed, 16 * 16 * 3);
}
