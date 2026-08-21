//! SPEC-0146 的完整文件逐 UTF-8 scalar 重复恢复矩阵。

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

fn duplicate_scalar(source: &str, start: usize, end: usize) -> String {
    let scalar = &source[start..end];
    let mut mutated = String::with_capacity(source.len() + scalar.len());
    mutated.push_str(&source[..end]);
    mutated.push_str(scalar);
    mutated.push_str(&source[end..]);
    mutated
}

fn parse_duplication(source: &str, context: &str) -> ParsedFile {
    let (sources, source_id, lexed) =
        lex_source_twice("parser-scalar-duplication.ko", source, context);
    parse_file_twice(&sources, source_id, source.len(), &lexed, context)
}

#[test]
fn duplicating_every_utf8_scalar_of_complete_files_is_recoverable() {
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
        let complete = parse_duplication(case.source, case.name);
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

        for boundary in boundaries.windows(2) {
            let start = boundary[0];
            let end = boundary[1];
            let context = format!("{} duplicate bytes {start}..{end}", case.name);
            let mutated = duplicate_scalar(case.source, start, end);
            assert_eq!(mutated.len(), case.source.len() + (end - start));
            parse_duplication(&mutated, &context);
            executed += 1;
        }
    }

    assert_eq!(executed, 1_351);
}
