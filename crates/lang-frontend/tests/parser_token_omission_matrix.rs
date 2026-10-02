//! SPEC-0080 / SPEC-0111 的合法完整语法逐显著 token 缺失恢复矩阵。

use lang_frontend::{
    parser::ParsedFile,
    source::{SourceMap, Span},
};

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/parser_grammar_corpus.rs"]
mod parser_grammar_corpus;
#[path = "support/parser_mutation_assertions.rs"]
mod parser_mutation_assertions;
#[path = "support/parser_mutation_owners.rs"]
mod parser_mutation_owners;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

use frontend_matrix_assertions::{lex_source_twice, parse_file_twice};
use parser_grammar_corpus::GRAMMAR_CASES;
use parser_mutation_assertions::assert_last_root_source;
use parser_mutation_owners::token_affects_owner;
use parser_mutation_tokens::{MutationSlot, original_token_slots};

const SENTINEL: &str = "val sentinel = 0";

fn baseline_and_omissions(case_source: &str, context: &str) -> (String, Vec<MutationSlot>) {
    let source = format!("{case_source}\n{SENTINEL}");
    let (sources, source_id, lexed) =
        lex_source_twice("parser-token-omission-baseline.ko", &source, context);
    assert!(
        lexed.diagnostics().is_empty(),
        "baseline must lex cleanly for {context}: {:?}",
        lexed.diagnostics()
    );

    let parsed = parse_file_twice(&sources, source_id, source.len(), &lexed, context);
    assert!(
        parsed.diagnostics().is_empty(),
        "baseline must parse cleanly for {context}: {:?}",
        parsed.diagnostics()
    );
    assert_sentinel(&sources, &parsed, context);

    let omissions = original_token_slots(&lexed, case_source.len());
    (source, omissions)
}

fn omit(source: &str, span: Span) -> String {
    let mut mutated = String::with_capacity(source.len() - (span.end() - span.start()));
    mutated.push_str(&source[..span.start()]);
    mutated.push_str(&source[span.end()..]);
    mutated
}

fn parse_mutation(source: &str, context: &str) -> (SourceMap, ParsedFile) {
    let (sources, source_id, lexed) = lex_source_twice("parser-token-omission.ko", source, context);
    let parsed = parse_file_twice(&sources, source_id, source.len(), &lexed, context);
    (sources, parsed)
}

fn assert_sentinel(sources: &SourceMap, parsed: &ParsedFile, context: &str) {
    assert_last_root_source(sources, parsed, SENTINEL, context);
}

#[test]
fn deleting_each_significant_token_is_total_and_recovers_non_owner_suffixes() {
    assert_eq!(GRAMMAR_CASES.len(), 22);

    let mut executed = 0;
    let mut owner_omissions = 0;
    let mut recoverable_omissions = 0;
    for case in GRAMMAR_CASES {
        let (source, omissions) = baseline_and_omissions(case.source, case.name);
        assert!(
            !omissions.is_empty(),
            "empty omission set for {}",
            case.name
        );

        for omission in omissions {
            let context = format!(
                "{} omit {:?} at {}..{}",
                case.name,
                omission.kind,
                omission.span.start(),
                omission.span.end()
            );
            let mutated = omit(&source, omission.span);
            let (sources, parsed) = parse_mutation(&mutated, &context);
            if token_affects_owner(omission.kind) {
                owner_omissions += 1;
            } else {
                assert_sentinel(&sources, &parsed, &context);
                recoverable_omissions += 1;
            }
            executed += 1;
        }
    }

    assert!(owner_omissions > 0);
    assert!(recoverable_omissions > 0);
    assert_eq!(
        (executed, owner_omissions, recoverable_omissions),
        (395, 96, 299)
    );
}

#[test]
fn missing_class_member_expression_preserves_the_class_closer_and_sentinel() {
    let source = "class Result { fun isOk(): Boolean = }\nval sentinel = 0";
    let (sources, parsed) = parse_mutation(source, "missing class member expression");
    assert_sentinel(&sources, &parsed, "missing class member expression");
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0009"]
    );
}

#[test]
fn expression_tail_recovery_skips_nested_lexical_owner_closers() {
    let source = r#"val text = "前${('界', "内${x}")}后" /*尾*/
val sentinel = 0"#;
    let (sources, parsed) = parse_mutation(source, "nested lexical owner in expression tail");
    assert_sentinel(&sources, &parsed, "nested lexical owner in expression tail");
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0010", "L0013"]
    );
}
