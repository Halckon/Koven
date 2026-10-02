//! SPEC-0083 / SPEC-0111 / SPEC-0157 的合法完整语法 token gap 词法 poison 插入矩阵。

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/parser_grammar_corpus.rs"]
mod parser_grammar_corpus;
#[path = "support/parser_lexical_poisons.rs"]
mod parser_lexical_poisons;
#[path = "support/parser_mutation_assertions.rs"]
mod parser_mutation_assertions;
#[path = "support/parser_mutation_gaps.rs"]
mod parser_mutation_gaps;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

use frontend_matrix_assertions::{lex_source_twice, parse_file_twice};
use parser_grammar_corpus::GRAMMAR_CASES;
use parser_lexical_poisons::LEXICAL_POISONS;
use parser_mutation_assertions::assert_last_root_source;
use parser_mutation_gaps::{Gap, token_gaps};
use parser_mutation_tokens::original_token_slots;

const SENTINEL: &str = "val sentinel = 0";

fn baseline_and_gaps(case_source: &str, context: &str) -> (String, Vec<Gap>, usize) {
    let source = format!("{case_source}\n{SENTINEL}");
    let (sources, source_id, lexed) =
        lex_source_twice("parser-poison-insertion-baseline.ko", &source, context);
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
    let gaps = token_gaps(slots.iter().map(|slot| (slot.kind, slot.span.end())));
    (source, gaps, slots.len())
}

fn insert_poison(source: &str, offset: usize, poison: &str) -> (String, (usize, usize)) {
    let poison_start = offset + 1;
    let poison_end = poison_start + poison.len();
    let mut mutated = String::with_capacity(source.len() + poison.len() + 2);
    mutated.push_str(&source[..offset]);
    mutated.push(' ');
    mutated.push_str(poison);
    mutated.push(' ');
    mutated.push_str(&source[offset..]);
    (mutated, (poison_start, poison_end))
}

#[test]
fn inserting_lexer_poison_at_each_token_gap_preserves_the_complete_grammar() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
    assert_eq!(LEXICAL_POISONS.len(), 4);

    let mut token_count = 0;
    let mut gap_count = 0;
    let mut code_mode_gaps = 0;
    let mut string_mode_gaps = 0;
    let mut executed = 0;
    let mut poison_counts = [0; 4];

    for case in GRAMMAR_CASES {
        let (source, gaps, case_token_count) = baseline_and_gaps(case.source, case.name);
        token_count += case_token_count;
        gap_count += gaps.len();

        for gap in gaps {
            if gap.code_mode {
                code_mode_gaps += 1;
            } else {
                string_mode_gaps += 1;
            }

            for (poison_index, poison) in LEXICAL_POISONS.iter().enumerate() {
                let context = format!(
                    "{} insert {} at gap {} ({})",
                    case.name,
                    poison.name,
                    gap.offset,
                    if gap.code_mode { "code" } else { "string" }
                );
                let (mutated, poison_range) = insert_poison(&source, gap.offset, poison.text);
                let (sources, source_id, lexed) =
                    lex_source_twice("parser-poison-insertion.ko", &mutated, &context);
                let poison_span = sources
                    .span(source_id, poison_range.0, poison_range.1)
                    .expect("inserted poison span must fit the mutation source");

                if gap.code_mode {
                    assert_eq!(
                        lexed
                            .diagnostics()
                            .iter()
                            .map(|diagnostic| {
                                (diagnostic.code().to_string(), diagnostic.primary_span())
                            })
                            .collect::<Vec<_>>(),
                        [(poison.code.to_owned(), poison_span)],
                        "target lexical root drift for {context}: {:?}",
                        lexed.diagnostics()
                    );
                } else {
                    assert!(
                        lexed.diagnostics().is_empty(),
                        "string text insertion must lex cleanly for {context}: {:?}",
                        lexed.diagnostics()
                    );
                }

                let parsed = parse_file_twice(&sources, source_id, mutated.len(), &lexed, &context);
                if !gap.code_mode {
                    assert!(
                        parsed.diagnostics().is_empty(),
                        "string text insertion must parse cleanly for {context}: {:?}",
                        parsed.diagnostics()
                    );
                }
                assert_last_root_source(&sources, &parsed, SENTINEL, &context);
                executed += 1;
                poison_counts[poison_index] += 1;
            }
        }
    }

    assert_eq!(
        (
            token_count,
            gap_count,
            code_mode_gaps,
            string_mode_gaps,
            executed,
        ),
        (395, 417, 408, 9, 1_668)
    );
    assert_eq!(poison_counts, [417; 4]);
}
