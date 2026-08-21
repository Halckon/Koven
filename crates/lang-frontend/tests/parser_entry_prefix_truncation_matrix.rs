//! SPEC-0085 的独立 Parser 入口逐 UTF-8 前缀 EOF 恢复矩阵。

use std::collections::BTreeSet;

use lang_frontend::{lexer::lex, source::SourceMap};

#[path = "support/parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "support/parser_prefixes.rs"]
mod parser_prefixes;

use parser_entry_matrix::{ENTRY_CASES, EntryCase, parse_entry_twice};
use parser_prefixes::prefix_ends;

fn parse_prefix(case: EntryCase, source: &str, context: &str) -> (usize, usize) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-entry-prefix-truncation.ko", source)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix prefix must lex internally");
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, context);
    (lexed.diagnostics().len(), parser_diagnostics)
}

#[test]
fn every_utf8_prefix_of_representative_entry_sources_is_recoverable() {
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
    let mut prefix_counts = [0; 3];
    for case in ENTRY_CASES {
        case_counts[case.kind.count_index()] += 1;
        let (lexer_diagnostics, parser_diagnostics) = parse_prefix(*case, case.source, case.name);
        assert_eq!(lexer_diagnostics, 0, "{} must lex cleanly", case.name);
        assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);

        let ends = prefix_ends(case.source);
        assert_eq!(ends.len(), case.source.chars().count() + 1);
        assert_eq!(ends.first(), Some(&0));
        assert_eq!(ends.last(), Some(&case.source.len()));
        assert!(ends.windows(2).all(|window| window[0] < window[1]));

        for end in ends {
            let context = format!("{} prefix byte {end}", case.name);
            parse_prefix(*case, &case.source[..end], &context);
            prefix_counts[case.kind.count_index()] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(prefix_counts, [195, 350, 202]);
    assert_eq!(prefix_counts.into_iter().sum::<usize>(), 747);
}
