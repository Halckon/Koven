//! 独立 Parser 入口 token mutation 矩阵共享的 baseline 与 slot 枚举。

use lang_frontend::{lexer::lex, source::SourceMap};

#[path = "parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

pub(crate) use parser_entry_matrix::{ENTRY_CASES, EntryCase, parse_entry_twice};
pub(crate) use parser_mutation_tokens::MutationSlot;

pub(crate) fn baseline_slots(case: EntryCase) -> Vec<MutationSlot> {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-entry-mutation-baseline.ko", case.source)
        .expect("baseline source name must be unique");
    let lexed = lex(&sources, source_id).expect("baseline must lex internally");
    assert!(
        lexed.diagnostics().is_empty(),
        "{}: {:?}",
        case.name,
        lexed.diagnostics()
    );
    let parser_diagnostics = parse_entry_twice(case, &sources, source_id, &lexed, case.name);
    assert_eq!(parser_diagnostics, 0, "{} must parse cleanly", case.name);
    parser_mutation_tokens::original_token_slots(&lexed, case.source.len())
}
