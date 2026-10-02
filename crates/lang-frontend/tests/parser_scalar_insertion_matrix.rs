//! SPEC-0149 的完整文件逐 UTF-8 scalar 边界固定字母表插入恢复矩阵。

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
use parser_scalar_replacements::SCALAR_REPLACEMENTS as SCALAR_INSERTIONS;

fn insert_scalar(source: &str, offset: usize, scalar: &str) -> String {
    let mut mutated = String::with_capacity(source.len() + scalar.len());
    mutated.push_str(&source[..offset]);
    mutated.push_str(scalar);
    mutated.push_str(&source[offset..]);
    mutated
}

fn parse_insertion(source: &str, context: &str) -> ParsedFile {
    let (sources, source_id, lexed) =
        lex_source_twice("parser-scalar-insertion.ko", source, context);
    parse_file_twice(&sources, source_id, source.len(), &lexed, context)
}

#[test]
fn inserting_the_fixed_scalar_alphabet_at_every_utf8_boundary_is_recoverable() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
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
        GRAMMAR_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        GRAMMAR_CASES.len()
    );

    let mut boundary_count = 0;
    let mut insertion_counts = [0; 13];
    for case in GRAMMAR_CASES {
        let complete = parse_insertion(case.source, case.name);
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
        boundary_count += boundaries.len();

        for offset in boundaries {
            for (insertion_index, insertion) in SCALAR_INSERTIONS.iter().enumerate() {
                let context = format!("{} insert {} at byte {offset}", case.name, insertion.name);
                let mutated = insert_scalar(case.source, offset, insertion.text);
                assert_eq!(mutated.len(), case.source.len() + insertion.text.len());
                assert_ne!(mutated, case.source);
                parse_insertion(&mutated, &context);
                insertion_counts[insertion_index] += 1;
            }
        }
    }

    assert_eq!(boundary_count, 1_366);
    assert_eq!(insertion_counts, [1_366; 13]);
    assert_eq!(insertion_counts.into_iter().sum::<usize>(), 17_758);
}
