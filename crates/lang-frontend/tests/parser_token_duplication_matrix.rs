//! SPEC-0082 / SPEC-0111 的合法完整语法逐显著 token 重复恢复矩阵。

use lang_frontend::source::Span;

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

use frontend_matrix_assertions::{lex_source_twice, parse_file_twice};
use parser_grammar_corpus::GRAMMAR_CASES;
use parser_mutation_assertions::assert_last_root_source;
use parser_mutation_lexemes::assert_exact_token;
use parser_mutation_modes::token_is_lexical_mode_segment;
use parser_mutation_owners::token_affects_owner;
use parser_mutation_tokens::{MutationSlot, original_token_slots};

const SENTINEL: &str = "val sentinel = 0";

fn baseline_and_slots(case_source: &str, context: &str) -> (String, Vec<MutationSlot>) {
    let source = format!("{case_source}\n{SENTINEL}");
    let (sources, source_id, lexed) =
        lex_source_twice("parser-token-duplication-baseline.ko", &source, context);
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
fn duplicating_each_significant_token_is_total_and_recovers_non_owner_suffixes() {
    assert_eq!(GRAMMAR_CASES.len(), 22);

    let mut executed = 0;
    let mut owner_mutations = 0;
    let mut recoverable_mutations = 0;
    let mut lexical_mode_mutations = 0;
    let mut exact_relexed_mutations = 0;

    for case in GRAMMAR_CASES {
        let (source, slots) = baseline_and_slots(case.source, case.name);
        assert!(!slots.is_empty(), "empty mutation slots for {}", case.name);

        for slot in slots {
            let context = format!(
                "{} duplicate {:?} at {}..{}",
                case.name,
                slot.kind,
                slot.span.start(),
                slot.span.end()
            );
            let (mutated, duplicate_span) = duplicate_token(&source, slot.span);
            let (sources, source_id, lexed) =
                lex_source_twice("parser-token-duplication.ko", &mutated, &context);

            if token_is_lexical_mode_segment(slot.kind) {
                lexical_mode_mutations += 1;
            } else {
                let original_span = sources
                    .span(source_id, slot.span.start(), slot.span.end())
                    .expect("original token span must fit the mutation source");
                let duplicate_span = sources
                    .span(source_id, duplicate_span.0, duplicate_span.1)
                    .expect("duplicate token span must fit the mutation source");
                assert_exact_token(&lexed, slot.kind, original_span, &context);
                assert_exact_token(&lexed, slot.kind, duplicate_span, &context);
                exact_relexed_mutations += 1;
            }

            let parsed = parse_file_twice(&sources, source_id, mutated.len(), &lexed, &context);
            if token_affects_owner(slot.kind) {
                owner_mutations += 1;
            } else {
                assert_last_root_source(&sources, &parsed, SENTINEL, &context);
                recoverable_mutations += 1;
            }
            executed += 1;
        }
    }

    assert!(owner_mutations > 0);
    assert!(recoverable_mutations > 0);
    assert!(lexical_mode_mutations > 0);
    assert!(exact_relexed_mutations > 0);
    assert_eq!(
        (
            executed,
            owner_mutations,
            recoverable_mutations,
            lexical_mode_mutations,
            exact_relexed_mutations,
        ),
        (396, 96, 300, 14, 382)
    );
}
