//! SPEC-0144 的完整文件逐 UTF-8 后缀恢复与公开产物不变量。

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
use parser_prefixes::prefix_ends as suffix_starts;

fn parse_suffix(source: &str, context: &str) -> ParsedFile {
    let (sources, source_id, lexed) =
        lex_source_twice("parser-suffix-truncation.ko", source, context);
    parse_file_twice(&sources, source_id, source.len(), &lexed, context)
}

#[test]
fn every_utf8_suffix_of_representative_complete_files_is_recoverable() {
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
        let complete = parse_suffix(case.source, case.name);
        assert!(
            complete.diagnostics().is_empty(),
            "complete source must parse cleanly for {}: {:?}",
            case.name,
            complete.diagnostics()
        );

        let starts = suffix_starts(case.source);
        assert_eq!(starts.len(), case.source.chars().count() + 1);
        assert_eq!(starts.first(), Some(&0));
        assert_eq!(starts.last(), Some(&case.source.len()));
        assert!(starts.windows(2).all(|window| window[0] < window[1]));
        for start in starts {
            let context = format!("{} suffix byte {start}", case.name);
            parse_suffix(&case.source[start..], &context);
            executed += 1;
        }
    }

    assert_eq!(executed, 1_366);
}
