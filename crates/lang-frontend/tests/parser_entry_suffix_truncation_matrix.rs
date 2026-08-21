//! SPEC-0144 的独立 Parser 入口逐 UTF-8 后缀恢复矩阵。

use std::collections::BTreeSet;

#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "support/parser_prefixes.rs"]
mod parser_prefixes;

use lexer_matrix_assertions::lex_source_twice;
use parser_entry_matrix::{ENTRY_CASES, EntryCase, parse_entry_twice, validate_lexed};
use parser_prefixes::prefix_ends as suffix_starts;

fn parse_suffix(case: EntryCase, source: &str, context: &str) -> (usize, usize) {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-entry-suffix-truncation.ko",
        source,
        context,
        validate_lexed,
    );
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, context);
    (lexed.diagnostics().len(), parser_diagnostics.0)
}

#[test]
fn every_utf8_suffix_of_representative_entry_sources_is_recoverable() {
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
    let mut suffix_counts = [0; 3];
    for case in ENTRY_CASES {
        case_counts[case.kind.count_index()] += 1;
        let (lexer_diagnostics, parser_diagnostics) = parse_suffix(*case, case.source, case.name);
        assert_eq!(lexer_diagnostics, 0, "{} must lex cleanly", case.name);
        assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);

        let starts = suffix_starts(case.source);
        assert_eq!(starts.len(), case.source.chars().count() + 1);
        assert_eq!(starts.first(), Some(&0));
        assert_eq!(starts.last(), Some(&case.source.len()));
        assert!(starts.windows(2).all(|window| window[0] < window[1]));

        for start in starts {
            let context = format!("{} suffix byte {start}", case.name);
            parse_suffix(*case, &case.source[start..], &context);
            suffix_counts[case.kind.count_index()] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(suffix_counts, [195, 350, 202]);
    assert_eq!(suffix_counts.into_iter().sum::<usize>(), 747);
}
