//! SPEC-0089 的独立 Parser 入口逐 token gap 词法 poison 插入矩阵。

use lang_frontend::{lexer::lex, source::SourceMap};

#[path = "support/parser_entry_mutation_support.rs"]
mod parser_entry_mutation_support;
#[path = "support/parser_lexical_poisons.rs"]
mod parser_lexical_poisons;
#[path = "support/parser_mutation_gaps.rs"]
mod parser_mutation_gaps;

use parser_entry_mutation_support::{ENTRY_CASES, baseline_slots, parse_entry_twice};
use parser_lexical_poisons::LEXICAL_POISONS;
use parser_mutation_gaps::token_gaps;

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
fn inserting_lexer_poison_at_each_token_gap_is_total_for_every_entry() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(LEXICAL_POISONS.len(), 2);
    let mut case_counts = [0; 3];
    let mut token_count = 0;
    let mut gap_counts = [0; 3];
    let mut mutation_counts = [0; 3];
    let mut poison_counts = [0; 2];
    let mut code_mode_gaps = 0;
    let mut string_mode_gaps = 0;

    for case in ENTRY_CASES {
        let entry_index = case.kind.count_index();
        case_counts[entry_index] += 1;
        let slots = baseline_slots(*case);
        let gaps = token_gaps(slots.iter().map(|slot| (slot.kind, slot.span.end())));
        assert_eq!(gaps.len(), slots.len() + 1);
        assert_eq!(gaps.first().map(|gap| gap.offset), Some(0));
        assert!(
            gaps.windows(2)
                .all(|window| window[0].offset < window[1].offset)
        );
        token_count += slots.len();
        gap_counts[entry_index] += gaps.len();

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
                let (mutated, poison_range) = insert_poison(case.source, gap.offset, poison.text);
                let mut sources = SourceMap::new();
                let source_id = sources
                    .add_source("parser-entry-lexical-poison-insertion.ko", &mutated)
                    .expect("mutation source name must be unique");
                let poison_span = sources
                    .span(source_id, poison_range.0, poison_range.1)
                    .expect("poison span must fit mutation source");
                let lexed = lex(&sources, source_id).expect("mutation must lex internally");

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
                        "string insertion must lex cleanly for {context}: {:?}",
                        lexed.diagnostics()
                    );
                }

                let parser_diagnostics =
                    parse_entry_twice(*case, &sources, source_id, &lexed, &context);
                if !gap.code_mode {
                    assert_eq!(
                        parser_diagnostics, 0,
                        "string insertion must parse cleanly for {context}"
                    );
                }
                mutation_counts[entry_index] += 1;
                poison_counts[poison_index] += 1;
            }
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(token_count, 240);
    assert_eq!(gap_counts, [70, 108, 74]);
    assert_eq!(mutation_counts, [140, 216, 148]);
    assert_eq!(poison_counts, [252, 252]);
    assert_eq!((code_mode_gaps, string_mode_gaps), (239, 13));
}
