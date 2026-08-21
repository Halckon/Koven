//! SPEC-0090 的独立 Parser 入口相邻显著 token 交换恢复矩阵。

use lang_frontend::{lexer::lex, source::SourceMap};

#[path = "support/parser_entry_mutation_support.rs"]
mod parser_entry_mutation_support;
#[path = "support/parser_mutation_lexemes.rs"]
mod parser_mutation_lexemes;
#[path = "support/parser_mutation_modes.rs"]
mod parser_mutation_modes;

use parser_entry_mutation_support::{ENTRY_CASES, MutationSlot, baseline_slots, parse_entry_twice};
use parser_mutation_lexemes::assert_exact_token;
use parser_mutation_modes::token_is_lexical_mode_segment;

fn transpose_pair(
    source: &str,
    left: MutationSlot,
    right: MutationSlot,
) -> (String, (usize, usize), (usize, usize)) {
    assert!(left.span.end() <= right.span.start());
    let left_source = &source[left.span.start()..left.span.end()];
    let right_source = &source[right.span.start()..right.span.end()];
    let right_start = left.span.start() + 1;
    let right_end = right_start + right_source.len();
    let left_start = right_end + 1;
    let left_end = left_start + left_source.len();

    let mut mutated = String::with_capacity(
        source.len() - (right.span.end() - left.span.start())
            + left_source.len()
            + right_source.len()
            + 3,
    );
    mutated.push_str(&source[..left.span.start()]);
    mutated.push(' ');
    mutated.push_str(right_source);
    mutated.push(' ');
    mutated.push_str(left_source);
    mutated.push(' ');
    mutated.push_str(&source[right.span.end()..]);
    (mutated, (right_start, right_end), (left_start, left_end))
}

#[test]
fn transposing_each_adjacent_token_pair_is_total_and_deterministic_for_every_entry() {
    assert_eq!(ENTRY_CASES.len(), 12);
    let mut case_counts = [0; 3];
    let mut token_counts = [0; 3];
    let mut mutation_counts = [0; 3];
    let mut lexical_mode_mutations = 0;
    let mut exact_relexed_mutations = 0;

    for case in ENTRY_CASES {
        let index = case.kind.count_index();
        case_counts[index] += 1;
        let slots = baseline_slots(*case);
        assert!(!slots.is_empty(), "empty token set for {}", case.name);
        token_counts[index] += slots.len();

        for pair in slots.windows(2) {
            let left = pair[0];
            let right = pair[1];
            let context = format!(
                "{} transpose {:?} at {}..{} with {:?} at {}..{}",
                case.name,
                left.kind,
                left.span.start(),
                left.span.end(),
                right.kind,
                right.span.start(),
                right.span.end()
            );
            let (mutated, right_range, left_range) = transpose_pair(case.source, left, right);
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("parser-entry-token-transposition.ko", &mutated)
                .expect("mutation source name must be unique");
            let lexed = lex(&sources, source_id).expect("mutation must lex internally");

            let affects_lexical_mode = token_is_lexical_mode_segment(left.kind)
                || token_is_lexical_mode_segment(right.kind);
            if affects_lexical_mode {
                lexical_mode_mutations += 1;
            } else {
                assert!(
                    lexed.diagnostics().is_empty(),
                    "ordinary swapped tokens must lex cleanly for {context}: {:?}",
                    lexed.diagnostics()
                );
                let right_span = sources
                    .span(source_id, right_range.0, right_range.1)
                    .expect("transposed right span must fit the mutation source");
                let left_span = sources
                    .span(source_id, left_range.0, left_range.1)
                    .expect("transposed left span must fit the mutation source");
                assert_exact_token(&lexed, right.kind, right_span, &context);
                assert_exact_token(&lexed, left.kind, left_span, &context);
                exact_relexed_mutations += 1;
            }

            parse_entry_twice(*case, &sources, source_id, &lexed, &context);
            mutation_counts[index] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(token_counts, [66, 104, 70]);
    assert_eq!(mutation_counts, [62, 100, 66]);
    assert_eq!((lexical_mode_mutations, exact_relexed_mutations), (27, 201));
}
