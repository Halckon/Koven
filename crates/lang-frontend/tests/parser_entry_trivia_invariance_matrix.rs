//! SPEC-0091 / SPEC-0101 / SPEC-0113 的独立入口 trivia 等价与词法分段不变量。

use lang_frontend::lexer::LexemeKind;

#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "support/parser_mutation_gaps.rs"]
mod parser_mutation_gaps;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;
#[path = "support/parser_trivia_variants.rs"]
mod parser_trivia_variants;

use lexer_matrix_assertions::lex_source_twice;
use parser_entry_matrix::{
    ENTRY_CASES, EntryCase, EntrySyntaxShape, parse_entry_twice, validate_lexed,
};
use parser_mutation_gaps::{Gap, token_gaps};
use parser_mutation_tokens::original_token_slots;
use parser_trivia_variants::{TRIVIA_VARIANTS, TriviaVariant, validate_inserted_trivia};

fn significant_kinds(lexemes: &[lang_frontend::lexer::Lexeme]) -> Vec<LexemeKind> {
    lexemes
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Trivia(_) => None,
            kind => Some(kind),
        })
        .collect()
}

fn insert_trivia(source: &str, offset: usize, trivia: &str) -> String {
    let mut mutated = String::with_capacity(source.len() + trivia.len());
    mutated.push_str(&source[..offset]);
    mutated.push_str(trivia);
    mutated.push_str(&source[offset..]);
    mutated
}

fn insert_trivia_at_all_gaps(source: &str, gaps: &[Gap], trivia: &str) -> String {
    let mut mutated = source.to_owned();
    for gap in gaps.iter().rev() {
        mutated.insert_str(gap.offset, trivia);
    }
    mutated
}

fn parse_clean(
    case: EntryCase,
    source: &str,
    insertions: &[(usize, TriviaVariant)],
    context: &str,
) -> (Vec<LexemeKind>, EntrySyntaxShape) {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-entry-trivia-invariance.ko",
        source,
        context,
        validate_lexed,
    );
    assert!(
        lexed.diagnostics().is_empty(),
        "Lexer diagnostics for {context}: {:?}\nsource={source:?}",
        lexed.diagnostics()
    );
    for (start, variant) in insertions {
        validate_inserted_trivia(&sources, &lexed, *start, *variant, context);
    }
    let kinds = significant_kinds(lexed.lexemes());
    let (diagnostic_count, shape) = parse_entry_twice(case, &sources, source_id, &lexed, context);
    assert_eq!(
        diagnostic_count, 0,
        "entry must parse cleanly for {context}"
    );
    (kinds, shape)
}

#[test]
fn non_newline_trivia_preserves_significant_tokens_and_syntax_shape_for_every_entry() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(TRIVIA_VARIANTS.len(), 3);
    let mut case_counts = [0; 3];
    let mut token_counts = [0; 3];
    let mut code_gap_counts = [0; 3];
    let mut mutation_counts = [0; 3];

    for case in ENTRY_CASES {
        let index = case.kind.count_index();
        case_counts[index] += 1;
        let (_, _, baseline_lexed) = lex_source_twice(
            "parser-entry-trivia-baseline.ko",
            case.source,
            case.name,
            validate_lexed,
        );
        let slots = original_token_slots(&baseline_lexed, case.source.len());
        token_counts[index] += slots.len();
        let gaps = token_gaps(slots.iter().map(|slot| (slot.kind, slot.span.end())));
        let code_gaps = gaps
            .into_iter()
            .filter(|gap| gap.code_mode)
            .collect::<Vec<_>>();
        code_gap_counts[index] += code_gaps.len();

        let (baseline_kinds, baseline_shape) = parse_clean(*case, case.source, &[], case.name);
        for gap in &code_gaps {
            for variant in TRIVIA_VARIANTS {
                let context = format!(
                    "{} insert {:?} at code gap {}",
                    case.name, variant.text, gap.offset
                );
                let mutated = insert_trivia(case.source, gap.offset, variant.text);
                let insertions = [(gap.offset, *variant)];
                let (kinds, shape) = parse_clean(*case, &mutated, &insertions, &context);
                assert_eq!(kinds, baseline_kinds, "token drift for {context}");
                assert_eq!(shape, baseline_shape, "syntax drift for {context}");
                mutation_counts[index] += 1;
            }
        }

        for variant in TRIVIA_VARIANTS {
            let context = format!("{} insert {:?} at all code gaps", case.name, variant.text);
            let mutated = insert_trivia_at_all_gaps(case.source, &code_gaps, variant.text);
            let insertions = code_gaps
                .iter()
                .enumerate()
                .map(|(index, gap)| (gap.offset + index * variant.text.len(), *variant))
                .collect::<Vec<_>>();
            let (kinds, shape) = parse_clean(*case, &mutated, &insertions, &context);
            assert_eq!(kinds, baseline_kinds, "token drift for {context}");
            assert_eq!(shape, baseline_shape, "syntax drift for {context}");
            mutation_counts[index] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(token_counts, [66, 104, 70]);
    assert_eq!(code_gap_counts, [66, 106, 67]);
    assert_eq!(mutation_counts, [210, 330, 213]);
    assert_eq!(mutation_counts.iter().sum::<usize>(), 753);
}
