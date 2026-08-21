//! SPEC-0087 的独立 Parser 入口逐显著 token 重复恢复矩阵。

use lang_frontend::{
    lexer::lex,
    source::{SourceMap, Span},
};

#[path = "support/parser_entry_mutation_support.rs"]
mod parser_entry_mutation_support;
#[path = "support/parser_mutation_lexemes.rs"]
mod parser_mutation_lexemes;
#[path = "support/parser_mutation_modes.rs"]
mod parser_mutation_modes;

use parser_entry_mutation_support::{ENTRY_CASES, baseline_slots, parse_entry_twice};
use parser_mutation_lexemes::assert_exact_token;
use parser_mutation_modes::token_is_lexical_mode_segment;

fn duplicate_token(source: &str, span: Span) -> (String, (usize, usize)) {
    let token_source = &source[span.start()..span.end()];
    let duplicate_start = span.end() + 1;
    let duplicate_end = duplicate_start + token_source.len();
    let mut mutated = String::with_capacity(source.len() + token_source.len() + 2);
    mutated.push_str(&source[..span.end()]);
    mutated.push(' ');
    mutated.push_str(token_source);
    mutated.push(' ');
    mutated.push_str(&source[span.end()..]);
    (mutated, (duplicate_start, duplicate_end))
}

#[test]
fn duplicating_each_significant_token_is_total_and_deterministic_for_every_entry() {
    assert_eq!(ENTRY_CASES.len(), 12);
    let mut case_counts = [0; 3];
    let mut mutation_counts = [0; 3];
    let mut lexical_mode_mutations = 0;
    let mut exact_relexed_mutations = 0;

    for case in ENTRY_CASES {
        case_counts[case.kind.count_index()] += 1;
        let slots = baseline_slots(*case);
        assert!(!slots.is_empty(), "empty token set for {}", case.name);

        for slot in slots {
            let context = format!(
                "{} duplicate {:?} at {}..{}",
                case.name,
                slot.kind,
                slot.span.start(),
                slot.span.end()
            );
            let (mutated, duplicate_span) = duplicate_token(case.source, slot.span);
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("parser-entry-token-duplication.ko", &mutated)
                .expect("mutation source name must be unique");
            let lexed = lex(&sources, source_id).expect("mutation must lex internally");

            if token_is_lexical_mode_segment(slot.kind) {
                lexical_mode_mutations += 1;
            } else {
                let original_span = sources
                    .span(source_id, slot.span.start(), slot.span.end())
                    .expect("original token span must fit mutation source");
                let duplicate_span = sources
                    .span(source_id, duplicate_span.0, duplicate_span.1)
                    .expect("duplicate token span must fit mutation source");
                assert_exact_token(&lexed, slot.kind, original_span, &context);
                assert_exact_token(&lexed, slot.kind, duplicate_span, &context);
                exact_relexed_mutations += 1;
            }

            parse_entry_twice(*case, &sources, source_id, &lexed, &context);
            mutation_counts[case.kind.count_index()] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(mutation_counts, [66, 104, 70]);
    assert_eq!((lexical_mode_mutations, exact_relexed_mutations), (20, 220));
    assert_eq!(lexical_mode_mutations + exact_relexed_mutations, 240);
}
