//! SPEC-0145 的独立 Parser 入口逐 UTF-8 内部区间删除恢复矩阵。

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

fn delete_range(source: &str, start: usize, end: usize) -> String {
    let mut mutated = String::with_capacity(source.len() - (end - start));
    mutated.push_str(&source[..start]);
    mutated.push_str(&source[end..]);
    mutated
}

fn parse_deletion(case: EntryCase, source: &str, context: &str) -> (usize, usize) {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-entry-interior-deletion.ko",
        source,
        context,
        validate_lexed,
    );
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, context);
    (lexed.diagnostics().len(), parser_diagnostics.0)
}

#[test]
fn every_utf8_aligned_interior_deletion_of_entry_sources_is_recoverable() {
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
    let mut deletion_counts = [0; 3];
    for case in ENTRY_CASES {
        let index = case.kind.count_index();
        case_counts[index] += 1;
        let (lexer_diagnostics, parser_diagnostics) = parse_deletion(*case, case.source, case.name);
        assert_eq!(lexer_diagnostics, 0, "{} must lex cleanly", case.name);
        assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);

        let boundaries = scalar_boundaries(case.source);
        assert_eq!(boundaries.len(), case.source.chars().count() + 1);
        assert_eq!(boundaries.first(), Some(&0));
        assert_eq!(boundaries.last(), Some(&case.source.len()));
        assert!(boundaries.windows(2).all(|window| window[0] < window[1]));

        for start_index in 1..boundaries.len() - 2 {
            for end_index in start_index + 1..boundaries.len() - 1 {
                let start = boundaries[start_index];
                let end = boundaries[end_index];
                let context = format!("{} delete bytes {start}..{end}", case.name);
                let mutated = delete_range(case.source, start, end);
                parse_deletion(*case, &mutated, &context);
                deletion_counts[index] += 1;
            }
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(deletion_counts, [4_159, 15_634, 4_656]);
    assert_eq!(deletion_counts.into_iter().sum::<usize>(), 24_449);
}
