//! SPEC-0145 的完整文件逐 UTF-8 内部区间删除恢复矩阵。

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

fn delete_range(source: &str, start: usize, end: usize) -> String {
    let mut mutated = String::with_capacity(source.len() - (end - start));
    mutated.push_str(&source[..start]);
    mutated.push_str(&source[end..]);
    mutated
}

fn parse_deletion(source: &str, context: &str) -> ParsedFile {
    let (sources, source_id, lexed) =
        lex_source_twice("parser-interior-deletion.ko", source, context);
    parse_file_twice(&sources, source_id, source.len(), &lexed, context)
}

#[test]
fn every_utf8_aligned_interior_deletion_of_complete_files_is_recoverable() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
    assert_eq!(
        GRAMMAR_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        GRAMMAR_CASES.len()
    );

    let mut executed = 0;
    for case in GRAMMAR_CASES {
        let complete = parse_deletion(case.source, case.name);
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

        for start_index in 1..boundaries.len() - 2 {
            for end_index in start_index + 1..boundaries.len() - 1 {
                let start = boundaries[start_index];
                let end = boundaries[end_index];
                let context = format!("{} delete bytes {start}..{end}", case.name);
                let mutated = delete_range(case.source, start, end);
                parse_deletion(&mutated, &context);
                executed += 1;
            }
        }
    }

    assert_eq!(executed, 44_584);
}
