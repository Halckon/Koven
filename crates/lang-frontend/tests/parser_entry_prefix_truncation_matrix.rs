//! SPEC-0085 的独立 Parser 入口逐 UTF-8 前缀 EOF 恢复矩阵。

use std::collections::BTreeSet;

use lang_frontend::{
    lexer::lex,
    parser::{parse_block, parse_declaration, parse_expression},
    source::{SourceId, SourceMap},
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/parser_prefixes.rs"]
mod parser_prefixes;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use parser_prefixes::prefix_ends;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EntryKind {
    Expression,
    Declaration,
    Block,
}

impl EntryKind {
    const fn count_index(self) -> usize {
        match self {
            Self::Expression => 0,
            Self::Declaration => 1,
            Self::Block => 2,
        }
    }
}

#[derive(Clone, Copy)]
struct EntryCase {
    name: &'static str,
    kind: EntryKind,
    source: &'static str,
}

const ENTRY_CASES: &[EntryCase] = &[
    EntryCase {
        name: "expression typed named mode call",
        kind: EntryKind::Expression,
        source: "service.send<Int>(name = borrow input, &target)",
    },
    EntryCase {
        name: "expression nested control flow",
        kind: EntryKind::Expression,
        source: "if (ready) yes else when (input) { is Type -> yes; else -> no }",
    },
    EntryCase {
        name: "expression unicode move lambda",
        kind: EntryKind::Expression,
        source: r#"move { left, right -> "前${left + right}后" }"#,
    },
    EntryCase {
        name: "expression operator hierarchy",
        kind: EntryKind::Expression,
        source: "target = a ?: b || c && d == e + f * g",
    },
    EntryCase {
        name: "declaration generic function",
        kind: EntryKind::Declaration,
        source: "fun <T: Copyable> id(borrow item: T): T = item",
    },
    EntryCase {
        name: "declaration ordinary class",
        kind: EntryKind::Declaration,
        source: "public class Box<T: Copyable>(private val item: T, var count: Int): Printable { override fun show(): Unit {} }",
    },
    EntryCase {
        name: "declaration enum variants",
        kind: EntryKind::Declaration,
        source: "enum class Result<T> { Ok(payload: T), Error(message: String); fun isOk(): Boolean = true }",
    },
    EntryCase {
        name: "declaration interface companion",
        kind: EntryKind::Declaration,
        source: "interface Printable { fun show(): Unit; companion object { const val NAME: String = \"printable\" } }",
    },
    EntryCase {
        name: "block local destructuring",
        kind: EntryKind::Block,
        source: "{ val (first, second) = pair return first + second }",
    },
    EntryCase {
        name: "block conditional return",
        kind: EntryKind::Block,
        source: "{\nif (ready) { yes } else { no }\nreturn\n}",
    },
    EntryCase {
        name: "block loop family",
        kind: EntryKind::Block,
        source: "{\nfor (item in items) { call(item) }\nloop { break }\n}",
    },
    EntryCase {
        name: "block unicode lexical owners",
        kind: EntryKind::Block,
        source: r#"{ val text = "前${call('界', "内${x}")}后" return text }"#,
    },
];

#[derive(Debug, PartialEq, Eq)]
struct EntryFingerprint {
    debug: String,
    diagnostic_count: usize,
}

fn parse_entry_once(
    case: EntryCase,
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &lang_frontend::lexer::LexedFile,
    context: &str,
) -> EntryFingerprint {
    match case.kind {
        EntryKind::Expression => {
            let parsed = parse_expression(sources, lexed)
                .unwrap_or_else(|error| panic!("expression parse failed for {context}: {error}"));
            assert_eq!(parsed.source_id(), source_id);
            validate_ast(source_id, source_len, parsed.ast());
            validate_diagnostics(source_id, source_len, parsed.diagnostics());
            parsed
                .ast()
                .expressions()
                .get(parsed.root())
                .unwrap_or_else(|error| panic!("expression root failed for {context}: {error}"));
            EntryFingerprint {
                debug: format!("{parsed:?}"),
                diagnostic_count: parsed.diagnostics().len(),
            }
        }
        EntryKind::Declaration => {
            let parsed = parse_declaration(sources, lexed)
                .unwrap_or_else(|error| panic!("declaration parse failed for {context}: {error}"));
            assert_eq!(parsed.source_id(), source_id);
            validate_ast(source_id, source_len, parsed.ast());
            validate_diagnostics(source_id, source_len, parsed.diagnostics());
            parsed
                .ast()
                .items()
                .get(parsed.root())
                .unwrap_or_else(|error| panic!("declaration root failed for {context}: {error}"));
            EntryFingerprint {
                debug: format!("{parsed:?}"),
                diagnostic_count: parsed.diagnostics().len(),
            }
        }
        EntryKind::Block => {
            let parsed = parse_block(sources, lexed)
                .unwrap_or_else(|error| panic!("block parse failed for {context}: {error}"));
            assert_eq!(parsed.source_id(), source_id);
            validate_ast(source_id, source_len, parsed.ast());
            validate_diagnostics(source_id, source_len, parsed.diagnostics());
            parsed
                .ast()
                .statements()
                .get(parsed.root())
                .unwrap_or_else(|error| panic!("block root failed for {context}: {error}"));
            EntryFingerprint {
                debug: format!("{parsed:?}"),
                diagnostic_count: parsed.diagnostics().len(),
            }
        }
    }
}

fn parse_entry_twice(case: EntryCase, source: &str, context: &str) -> (usize, usize) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-entry-prefix-truncation.ko", source)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix prefix must lex internally");
    validate_lexed(source_id, source.len(), &lexed);

    let first = parse_entry_once(case, &sources, source_id, source.len(), &lexed, context);
    let repeated = parse_entry_once(case, &sources, source_id, source.len(), &lexed, context);
    assert_eq!(first, repeated, "non-deterministic parse for {context}");
    (lexed.diagnostics().len(), first.diagnostic_count)
}

#[test]
fn every_utf8_prefix_of_representative_entry_sources_is_recoverable() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(
        ENTRY_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        ENTRY_CASES.len()
    );

    let mut case_counts = [0; 3];
    let mut prefix_counts = [0; 3];
    for case in ENTRY_CASES {
        case_counts[case.kind.count_index()] += 1;
        let (lexer_diagnostics, parser_diagnostics) =
            parse_entry_twice(*case, case.source, case.name);
        assert_eq!(lexer_diagnostics, 0, "{} must lex cleanly", case.name);
        assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);

        let ends = prefix_ends(case.source);
        assert_eq!(ends.len(), case.source.chars().count() + 1);
        assert_eq!(ends.first(), Some(&0));
        assert_eq!(ends.last(), Some(&case.source.len()));
        assert!(ends.windows(2).all(|window| window[0] < window[1]));

        for end in ends {
            let context = format!("{} prefix byte {end}", case.name);
            parse_entry_twice(*case, &case.source[..end], &context);
            prefix_counts[case.kind.count_index()] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(prefix_counts, [195, 350, 202]);
    assert_eq!(prefix_counts.into_iter().sum::<usize>(), 747);
}
