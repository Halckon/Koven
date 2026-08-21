//! SPEC-0086 的独立 Parser 入口逐显著 token 缺失恢复矩阵。

use lang_frontend::{
    lexer::lex,
    source::{SourceMap, Span},
};

#[path = "support/parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

use parser_entry_matrix::{ENTRY_CASES, EntryCase, parse_entry_twice};
use parser_mutation_tokens::{MutationSlot, original_token_slots};

fn baseline_slots(case: EntryCase) -> Vec<MutationSlot> {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-entry-omission-baseline.ko", case.source)
        .expect("baseline source name must be unique");
    let lexed = lex(&sources, source_id).expect("baseline must lex internally");
    assert!(
        lexed.diagnostics().is_empty(),
        "{}: {:?}",
        case.name,
        lexed.diagnostics()
    );
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, case.name);
    assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);
    original_token_slots(&lexed, case.source.len())
}

fn omit(source: &str, span: Span) -> String {
    let mut mutated = String::with_capacity(source.len() - (span.end() - span.start()));
    mutated.push_str(&source[..span.start()]);
    mutated.push_str(&source[span.end()..]);
    mutated
}

fn parse_mutation(case: EntryCase, source: &str, context: &str) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-entry-token-omission.ko", source)
        .expect("mutation source name must be unique");
    let lexed = lex(&sources, source_id).expect("mutation must lex internally");
    parse_entry_twice(case, &sources, source_id, &lexed, context);
}

#[test]
fn deleting_each_significant_token_is_total_and_deterministic_for_every_entry() {
    assert_eq!(ENTRY_CASES.len(), 12);
    let mut case_counts = [0; 3];
    let mut mutation_counts = [0; 3];

    for case in ENTRY_CASES {
        case_counts[case.kind.count_index()] += 1;
        let slots = baseline_slots(*case);
        assert!(!slots.is_empty(), "empty token set for {}", case.name);

        for slot in slots {
            let context = format!(
                "{} omit {:?} at {}..{}",
                case.name,
                slot.kind,
                slot.span.start(),
                slot.span.end()
            );
            let mutated = omit(case.source, slot.span);
            assert_eq!(
                mutated.len(),
                case.source.len() - (slot.span.end() - slot.span.start())
            );
            parse_mutation(*case, &mutated, &context);
            mutation_counts[case.kind.count_index()] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(mutation_counts, [66, 104, 70]);
    assert_eq!(mutation_counts.into_iter().sum::<usize>(), 240);
}
