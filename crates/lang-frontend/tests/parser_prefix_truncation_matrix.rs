//! SPEC-0079 / SPEC-0099 / SPEC-0111 的 UTF-8 前缀恢复与共享文件产物不变量。

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
use parser_prefixes::prefix_ends;

fn parse_prefix(source: &str, context: &str) -> ParsedFile {
    let (sources, source_id, lexed) =
        lex_source_twice("parser-prefix-truncation.ko", source, context);
    parse_file_twice(&sources, source_id, source.len(), &lexed, context)
}

#[test]
fn every_utf8_prefix_of_representative_complete_files_is_recoverable() {
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
        let complete = parse_prefix(case.source, case.name);
        assert!(
            complete.diagnostics().is_empty(),
            "complete source must parse cleanly for {}: {:?}",
            case.name,
            complete.diagnostics()
        );

        let ends = prefix_ends(case.source);
        assert_eq!(ends.len(), case.source.chars().count() + 1);
        assert_eq!(ends.first(), Some(&0));
        assert_eq!(ends.last(), Some(&case.source.len()));
        assert!(ends.windows(2).all(|window| window[0] < window[1]));
        for end in ends {
            let context = format!("{} prefix byte {end}", case.name);
            parse_prefix(&case.source[..end], &context);
            executed += 1;
        }
    }

    assert_eq!(executed, 1_366);
}
