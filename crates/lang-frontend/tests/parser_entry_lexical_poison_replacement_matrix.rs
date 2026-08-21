//! SPEC-0088 / SPEC-0114 / SPEC-0157 的独立 Parser 入口逐显著 token 词法 poison 替换矩阵。

use lang_frontend::source::Span;

#[path = "support/parser_entry_mutation_support.rs"]
mod parser_entry_mutation_support;
#[path = "support/parser_lexical_poisons.rs"]
mod parser_lexical_poisons;
#[path = "support/parser_mutation_modes.rs"]
mod parser_mutation_modes;

use parser_entry_mutation_support::{
    ENTRY_CASES, baseline_slots, lex_source_twice, parse_entry_twice,
};
use parser_lexical_poisons::LEXICAL_POISONS;
use parser_mutation_modes::token_is_lexical_mode_segment;

fn replace_with_poison(source: &str, span: Span, poison: &str) -> (String, (usize, usize)) {
    let poison_start = span.start() + 1;
    let poison_end = poison_start + poison.len();
    let mut mutated =
        String::with_capacity(source.len() - (span.end() - span.start()) + poison.len() + 2);
    mutated.push_str(&source[..span.start()]);
    mutated.push(' ');
    mutated.push_str(poison);
    mutated.push(' ');
    mutated.push_str(&source[span.end()..]);
    (mutated, (poison_start, poison_end))
}

#[test]
fn replacing_each_significant_token_with_lexer_poison_is_total_for_every_entry() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(LEXICAL_POISONS.len(), 4);
    let mut case_counts = [0; 3];
    let mut mutation_counts = [0; 3];
    let mut poison_counts = [0; 4];
    let mut lexical_mode_mutations = 0;
    let mut target_code_mutations = 0;

    for case in ENTRY_CASES {
        case_counts[case.kind.count_index()] += 1;
        let slots = baseline_slots(*case);
        assert!(!slots.is_empty(), "empty token set for {}", case.name);

        for slot in slots {
            for (poison_index, poison) in LEXICAL_POISONS.iter().enumerate() {
                let context = format!(
                    "{} replace {:?} at {}..{} with {}",
                    case.name,
                    slot.kind,
                    slot.span.start(),
                    slot.span.end(),
                    poison.name
                );
                let (mutated, poison_span) =
                    replace_with_poison(case.source, slot.span, poison.text);
                let (sources, source_id, lexed) = lex_source_twice(
                    "parser-entry-lexical-poison-replacement.ko",
                    &mutated,
                    &context,
                );

                if token_is_lexical_mode_segment(slot.kind) {
                    lexical_mode_mutations += 1;
                } else {
                    let poison_span = sources
                        .span(source_id, poison_span.0, poison_span.1)
                        .expect("poison span must fit mutation source");
                    assert_eq!(
                        lexed
                            .diagnostics()
                            .iter()
                            .filter(|diagnostic| {
                                diagnostic.code().to_string() == poison.code
                                    && diagnostic.primary_span() == poison_span
                            })
                            .count(),
                        1,
                        "target lexical root drift for {context}: {:?}",
                        lexed.diagnostics()
                    );
                    target_code_mutations += 1;
                }

                parse_entry_twice(*case, &sources, source_id, &lexed, &context);
                mutation_counts[case.kind.count_index()] += 1;
                poison_counts[poison_index] += 1;
            }
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(mutation_counts, [264, 416, 280]);
    assert_eq!(poison_counts, [240; 4]);
    assert_eq!((lexical_mode_mutations, target_code_mutations), (80, 880));
}
