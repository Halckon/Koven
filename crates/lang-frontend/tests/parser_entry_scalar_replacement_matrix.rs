//! SPEC-0148 的独立 Parser 入口逐 UTF-8 scalar 固定字母表替换恢复矩阵。

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
use parser_scalar_replacements::SCALAR_REPLACEMENTS;

fn replace_scalar(source: &str, start: usize, end: usize, replacement: &str) -> String {
    let mut mutated = String::with_capacity(source.len() - (end - start) + replacement.len());
    mutated.push_str(&source[..start]);
    mutated.push_str(replacement);
    mutated.push_str(&source[end..]);
    mutated
}

fn parse_replacement(case: EntryCase, source: &str, context: &str) -> (usize, usize) {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-entry-scalar-replacement.ko",
        source,
        context,
        validate_lexed,
    );
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, context);
    (lexed.diagnostics().len(), parser_diagnostics.0)
}

#[test]
fn replacing_every_utf8_scalar_of_entry_sources_with_the_fixed_alphabet_is_recoverable() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(SCALAR_REPLACEMENTS.len(), 13);
    assert_eq!(
        SCALAR_REPLACEMENTS
            .iter()
            .map(|replacement| replacement.text)
            .collect::<BTreeSet<_>>()
            .len(),
        SCALAR_REPLACEMENTS.len()
    );
    assert!(
        SCALAR_REPLACEMENTS
            .iter()
            .all(|replacement| replacement.text.chars().count() == 1)
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
    let mut candidate_counts = [0; 3];
    let mut noop_counts = [0; 3];
    let mut replacement_counts = [0; 13];
    for case in ENTRY_CASES {
        let index = case.kind.count_index();
        case_counts[index] += 1;
        let (lexer_diagnostics, parser_diagnostics) =
            parse_replacement(*case, case.source, case.name);
        assert_eq!(lexer_diagnostics, 0, "{} must lex cleanly", case.name);
        assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);

        let boundaries = scalar_boundaries(case.source);
        assert_eq!(boundaries.len(), case.source.chars().count() + 1);
        assert!(boundaries.windows(2).all(|window| window[0] < window[1]));
        for boundary in boundaries.windows(2) {
            let start = boundary[0];
            let end = boundary[1];
            for (replacement_index, replacement) in SCALAR_REPLACEMENTS.iter().enumerate() {
                candidate_counts[index] += 1;
                if &case.source[start..end] == replacement.text {
                    noop_counts[index] += 1;
                    continue;
                }
                let context = format!(
                    "{} replace bytes {start}..{end} with {}",
                    case.name, replacement.name
                );
                let mutated = replace_scalar(case.source, start, end, replacement.text);
                assert_ne!(mutated, case.source);
                parse_replacement(*case, &mutated, &context);
                replacement_counts[replacement_index] += 1;
            }
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(candidate_counts, [2_392, 4_498, 2_574]);
    assert_eq!(noop_counts, [15, 29, 42]);
    assert_eq!(
        replacement_counts,
        [
            699, 728, 728, 720, 726, 728, 725, 728, 727, 710, 710, 722, 727
        ]
    );
    assert_eq!(replacement_counts.into_iter().sum::<usize>(), 9_378);
}
