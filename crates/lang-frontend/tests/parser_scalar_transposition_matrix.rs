//! SPEC-0147 的完整文件相邻 UTF-8 scalar 交换恢复矩阵。

use std::collections::BTreeSet;

use lang_frontend::parser::ParsedFile;

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/parser_grammar_corpus.rs"]
mod parser_grammar_corpus;
#[path = "support/parser_prefixes.rs"]
mod parser_prefixes;

use frontend_matrix_assertions::{lex_source_twice, parse_file_twice};
use parser_grammar_corpus::GRAMMAR_CASES;
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

fn parse_transposition(source: &str, context: &str) -> ParsedFile {
    let (sources, source_id, lexed) =
        lex_source_twice("parser-scalar-transposition.ko", source, context);
    parse_file_twice(&sources, source_id, source.len(), &lexed, context)
}

#[test]
fn transposing_every_distinct_adjacent_utf8_scalar_pair_is_recoverable() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
    assert_eq!(
        GRAMMAR_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        GRAMMAR_CASES.len()
    );

    let mut candidates = 0;
    let mut noops = 0;
    let mut executed = 0;
    for case in GRAMMAR_CASES {
        let complete = parse_transposition(case.source, case.name);
        assert!(
            complete.diagnostics().is_empty(),
            "complete source must parse cleanly for {}: {:?}",
            case.name,
            complete.diagnostics()
        );

        let boundaries = scalar_boundaries(case.source);
        assert_eq!(boundaries.len(), case.source.chars().count() + 1);
        assert_eq!(boundaries.first(), Some(&0));
        assert_eq!(boundaries.last(), Some(&case.source.len()));
        assert!(boundaries.windows(2).all(|window| window[0] < window[1]));

        for boundary in boundaries.windows(3) {
            let start = boundary[0];
            let middle = boundary[1];
            let end = boundary[2];
            candidates += 1;
            if case.source[start..middle] == case.source[middle..end] {
                noops += 1;
                continue;
            }
            let context = format!("{} transpose bytes {start}..{middle}..{end}", case.name);
            let mutated = transpose_scalars(case.source, start, middle, end);
            assert_eq!(mutated.len(), case.source.len());
            assert_ne!(mutated, case.source);
            parse_transposition(&mutated, &context);
            executed += 1;
        }
    }

    assert_eq!((candidates, noops, executed), (1_329, 25, 1_304));
}
