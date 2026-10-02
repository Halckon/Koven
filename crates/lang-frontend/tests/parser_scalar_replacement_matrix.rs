//! SPEC-0148 的完整文件逐 UTF-8 scalar 固定字母表替换恢复矩阵。

use std::collections::BTreeSet;

use lang_frontend::parser::ParsedFile;

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/parser_grammar_corpus.rs"]
mod parser_grammar_corpus;
#[path = "support/parser_prefixes.rs"]
mod parser_prefixes;
#[path = "support/parser_scalar_replacements.rs"]
mod parser_scalar_replacements;

use frontend_matrix_assertions::{lex_source_twice, parse_file_twice};
use parser_grammar_corpus::GRAMMAR_CASES;
use parser_prefixes::prefix_ends as scalar_boundaries;
use parser_scalar_replacements::SCALAR_REPLACEMENTS;

fn replace_scalar(source: &str, start: usize, end: usize, replacement: &str) -> String {
    let mut mutated = String::with_capacity(source.len() - (end - start) + replacement.len());
    mutated.push_str(&source[..start]);
    mutated.push_str(replacement);
    mutated.push_str(&source[end..]);
    mutated
}

fn parse_replacement(source: &str, context: &str) -> ParsedFile {
    let (sources, source_id, lexed) =
        lex_source_twice("parser-scalar-replacement.ko", source, context);
    parse_file_twice(&sources, source_id, source.len(), &lexed, context)
}

#[test]
fn replacing_every_utf8_scalar_with_the_fixed_alphabet_is_recoverable() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
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
        GRAMMAR_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        GRAMMAR_CASES.len()
    );

    let mut candidates = 0;
    let mut noops = 0;
    let mut replacement_counts = [0; 13];
    for case in GRAMMAR_CASES {
        let complete = parse_replacement(case.source, case.name);
        assert!(
            complete.diagnostics().is_empty(),
            "complete source must parse cleanly for {}: {:?}",
            case.name,
            complete.diagnostics()
        );

        let boundaries = scalar_boundaries(case.source);
        assert_eq!(boundaries.len(), case.source.chars().count() + 1);
        assert!(boundaries.windows(2).all(|window| window[0] < window[1]));
        for boundary in boundaries.windows(2) {
            let start = boundary[0];
            let end = boundary[1];
            for (replacement_index, replacement) in SCALAR_REPLACEMENTS.iter().enumerate() {
                candidates += 1;
                if &case.source[start..end] == replacement.text {
                    noops += 1;
                    continue;
                }
                let context = format!(
                    "{} replace bytes {start}..{end} with {}",
                    case.name, replacement.name
                );
                let mutated = replace_scalar(case.source, start, end, replacement.text);
                assert_ne!(mutated, case.source);
                parse_replacement(&mutated, &context);
                replacement_counts[replacement_index] += 1;
            }
        }
    }

    assert_eq!((candidates, noops), (17_472, 123));
    assert_eq!(
        replacement_counts,
        [
            1_274, 1_342, 1_344, 1_338, 1_342, 1_344, 1_342, 1_342, 1_340, 1_327, 1_327, 1_344,
            1_343,
        ]
    );
    assert_eq!(replacement_counts.into_iter().sum::<usize>(), 17_349);
}
