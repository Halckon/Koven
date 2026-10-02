//! SPEC-0147 的独立 Parser 入口相邻 UTF-8 scalar 交换恢复矩阵。

use std::collections::BTreeSet;

#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "support/parser_prefixes.rs"]
mod parser_prefixes;

use lexer_matrix_assertions::lex_source_twice;
use parser_entry_matrix::{ENTRY_CASES, EntryCase, parse_entry_twice, validate_lexed};
use parser_prefixes::prefix_ends as scalar_boundaries;

fn transpose_scalars(source: &str, start: usize, middle: usize, end: usize) -> String {
    let left = &source[start..middle];
    let right = &source[middle..end];
    let mut mutated = String::with_capacity(source.len());
    mutated.push_str(&source[..start]);
    mutated.push_str(right);
    mutated.push_str(left);
    mutated.push_str(&source[end..]);
    mutated
}

fn parse_transposition(case: EntryCase, source: &str, context: &str) -> (usize, usize) {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-entry-scalar-transposition.ko",
        source,
        context,
        validate_lexed,
    );
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, context);
    (lexed.diagnostics().len(), parser_diagnostics.0)
}

#[test]
fn transposing_every_distinct_adjacent_utf8_scalar_pair_of_entry_sources_is_recoverable() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(
        ENTRY_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        ENTRY_CASES.len()
    );

    let mut case_counts = [0; 3];
    let mut candidate_counts = [0; 3];
    let mut noop_counts = [0; 3];
    let mut transposition_counts = [0; 3];
    for case in ENTRY_CASES {
        let index = case.kind.count_index();
        case_counts[index] += 1;
        let (lexer_diagnostics, parser_diagnostics) =
            parse_transposition(*case, case.source, case.name);
        assert_eq!(lexer_diagnostics, 0, "{} must lex cleanly", case.name);
        assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);

        let boundaries = scalar_boundaries(case.source);
        assert_eq!(boundaries.len(), case.source.chars().count() + 1);
        assert_eq!(boundaries.first(), Some(&0));
        assert_eq!(boundaries.last(), Some(&case.source.len()));
        assert!(boundaries.windows(2).all(|window| window[0] < window[1]));

        for boundary in boundaries.windows(3) {
            let start = boundary[0];
            let middle = boundary[1];
            let end = boundary[2];
            candidate_counts[index] += 1;
            if case.source[start..middle] == case.source[middle..end] {
                noop_counts[index] += 1;
                continue;
            }
            let context = format!("{} transpose bytes {start}..{middle}..{end}", case.name);
            let mutated = transpose_scalars(case.source, start, middle, end);
            assert_eq!(mutated.len(), case.source.len());
            assert_ne!(mutated, case.source);
            parse_transposition(*case, &mutated, &context);
            transposition_counts[index] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(candidate_counts, [180, 342, 194]);
    assert_eq!(noop_counts, [3, 7, 3]);
    assert_eq!(transposition_counts, [177, 335, 191]);
    assert_eq!(transposition_counts.into_iter().sum::<usize>(), 703);
}
