//! SPEC-0149 的独立 Parser 入口逐 UTF-8 scalar 边界固定字母表插入恢复矩阵。

use std::collections::BTreeSet;

#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "support/parser_prefixes.rs"]
mod parser_prefixes;
#[path = "support/parser_scalar_replacements.rs"]
mod parser_scalar_replacements;

use lexer_matrix_assertions::lex_source_twice;
use parser_entry_matrix::{ENTRY_CASES, EntryCase, parse_entry_twice, validate_lexed};
use parser_prefixes::prefix_ends as scalar_boundaries;
use parser_scalar_replacements::SCALAR_REPLACEMENTS as SCALAR_INSERTIONS;

fn insert_scalar(source: &str, offset: usize, scalar: &str) -> String {
    let mut mutated = String::with_capacity(source.len() + scalar.len());
    mutated.push_str(&source[..offset]);
    mutated.push_str(scalar);
    mutated.push_str(&source[offset..]);
    mutated
}

fn parse_insertion(case: EntryCase, source: &str, context: &str) -> (usize, usize) {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-entry-scalar-insertion.ko",
        source,
        context,
        validate_lexed,
    );
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, context);
    (lexed.diagnostics().len(), parser_diagnostics.0)
}

#[test]
fn inserting_the_fixed_scalar_alphabet_at_every_entry_utf8_boundary_is_recoverable() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(SCALAR_INSERTIONS.len(), 13);
    assert_eq!(
        SCALAR_INSERTIONS
            .iter()
            .map(|insertion| insertion.text)
            .collect::<BTreeSet<_>>()
            .len(),
        SCALAR_INSERTIONS.len()
    );
    assert!(
        SCALAR_INSERTIONS
            .iter()
            .all(|insertion| insertion.text.chars().count() == 1)
    );
    assert_eq!(
        ENTRY_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        ENTRY_CASES.len()
    );

    let mut case_counts = [0; 3];
    let mut boundary_counts = [0; 3];
    let mut insertion_counts = [0; 3];
    let mut scalar_counts = [0; 13];
    for case in ENTRY_CASES {
        let index = case.kind.count_index();
        case_counts[index] += 1;
        let (lexer_diagnostics, parser_diagnostics) =
            parse_insertion(*case, case.source, case.name);
        assert_eq!(lexer_diagnostics, 0, "{} must lex cleanly", case.name);
        assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);

        let boundaries = scalar_boundaries(case.source);
        assert_eq!(boundaries.len(), case.source.chars().count() + 1);
        assert_eq!(boundaries.first(), Some(&0));
        assert_eq!(boundaries.last(), Some(&case.source.len()));
        assert!(boundaries.windows(2).all(|window| window[0] < window[1]));
        boundary_counts[index] += boundaries.len();

        for offset in boundaries {
            for (scalar_index, insertion) in SCALAR_INSERTIONS.iter().enumerate() {
                let context = format!("{} insert {} at byte {offset}", case.name, insertion.name);
                let mutated = insert_scalar(case.source, offset, insertion.text);
                assert_eq!(mutated.len(), case.source.len() + insertion.text.len());
                assert_ne!(mutated, case.source);
                parse_insertion(*case, &mutated, &context);
                insertion_counts[index] += 1;
                scalar_counts[scalar_index] += 1;
            }
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(boundary_counts, [195, 350, 202]);
    assert_eq!(insertion_counts, [2_535, 4_550, 2_626]);
    assert_eq!(scalar_counts, [747; 13]);
    assert_eq!(insertion_counts.into_iter().sum::<usize>(), 9_711);
}
