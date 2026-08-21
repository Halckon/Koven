//! SPEC-0084 的合法完整语法相邻显著 token 交换恢复矩阵。

use lang_frontend::{lexer::lex, source::SourceMap};

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/parser_grammar_corpus.rs"]
mod parser_grammar_corpus;
#[path = "support/parser_mutation_assertions.rs"]
mod parser_mutation_assertions;
#[path = "support/parser_mutation_lexemes.rs"]
mod parser_mutation_lexemes;
#[path = "support/parser_mutation_modes.rs"]
mod parser_mutation_modes;
#[path = "support/parser_mutation_owners.rs"]
mod parser_mutation_owners;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

use frontend_matrix_assertions::{parse_file_twice, validate_lexed};
use parser_grammar_corpus::GRAMMAR_CASES;
use parser_mutation_assertions::assert_last_root_source;
use parser_mutation_lexemes::assert_exact_token;
use parser_mutation_modes::token_is_lexical_mode_segment;
use parser_mutation_owners::token_affects_owner;
use parser_mutation_tokens::{MutationSlot, original_token_slots};

const SENTINEL: &str = "val sentinel = 0";

fn baseline_and_slots(case_source: &str, context: &str) -> (String, Vec<MutationSlot>) {
    let source = format!("{case_source}\n{SENTINEL}");
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-token-transposition-baseline.ko", &source)
        .expect("baseline source name must be unique");
    let lexed = lex(&sources, source_id).expect("baseline source must lex internally");
    validate_lexed(source_id, source.len(), &lexed);
    assert!(
        lexed.diagnostics().is_empty(),
        "baseline must lex cleanly for {context}: {:?}",
        lexed.diagnostics()
    );
    let parsed = parse_file_twice(&sources, source_id, source.len(), &lexed, context);
    assert!(
        parsed.diagnostics().is_empty(),
        "baseline must parse cleanly for {context}: {:?}",
        parsed.diagnostics()
    );
    assert_last_root_source(&sources, &parsed, SENTINEL, context);
    let slots = original_token_slots(&lexed, case_source.len());
    (source, slots)
}

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
fn transposing_each_adjacent_token_pair_is_total_and_recovers_non_owner_suffixes() {
    assert_eq!(GRAMMAR_CASES.len(), 22);

    let mut token_count = 0;
    let mut executed = 0;
    let mut lexical_mode_mutations = 0;
    let mut exact_relexed_mutations = 0;
    let mut owner_mutations = 0;
    let mut recoverable_mutations = 0;

    for case in GRAMMAR_CASES {
        let (source, slots) = baseline_and_slots(case.source, case.name);
        token_count += slots.len();

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
            let (mutated, right_range, left_range) = transpose_pair(&source, left, right);
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("parser-token-transposition.ko", &mutated)
                .expect("mutation source name must be unique");
            let lexed = lex(&sources, source_id).expect("mutation must lex internally");
            validate_lexed(source_id, mutated.len(), &lexed);

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

            let parsed = parse_file_twice(&sources, source_id, mutated.len(), &lexed, &context);
            let affects_owner = token_affects_owner(left.kind) || token_affects_owner(right.kind);
            if affects_owner {
                owner_mutations += 1;
            } else {
                assert_last_root_source(&sources, &parsed, SENTINEL, &context);
                recoverable_mutations += 1;
            }
            executed += 1;
        }
    }

    assert_eq!(
        (
            token_count,
            executed,
            lexical_mode_mutations,
            exact_relexed_mutations,
            owner_mutations,
            recoverable_mutations,
        ),
        (396, 374, 18, 356, 154, 220)
    );
}
