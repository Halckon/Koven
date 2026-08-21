//! SPEC-0153 的四公开 Parser 入口小调用栈隔离矩阵。

use std::thread;

use lang_frontend::{
    parser::{ParserInternalError, parse_block, parse_declaration, parse_expression, parse_file},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    assert_parser_error_twice, lex_parser_source_twice, parse_block_twice, parse_declaration_twice,
    parse_expression_twice, parse_file_twice,
};

const SMALL_CALLER_STACK: usize = 64 * 1_024;
const RECURSION_LIMIT: usize = 1_024;

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("stack-isolation.ko", source)
        .expect("stack-isolation source name must be unique");
    (sources, source_id)
}

fn run_on_small_caller(label: &str, action: impl FnOnce() + Send + 'static) {
    thread::Builder::new()
        .name(format!("{label}-small-caller"))
        .stack_size(SMALL_CALLER_STACK)
        .spawn(action)
        .unwrap_or_else(|error| panic!("{label} small caller thread must start: {error}"))
        .join()
        .unwrap_or_else(|_| panic!("{label} parser must isolate work from the caller stack"));
}

#[test]
fn every_public_parser_entry_isolates_boundary_work_from_a_small_caller_stack() {
    run_on_small_caller("expression", || {
        let grouped = |depth| format!("{}x{}", "(".repeat(depth), ")".repeat(depth));
        let (sources, source_id) = add_source(grouped(511));
        let parsed = parse_expression_twice(&sources, source_id, "small-stack expression success");
        assert!(parsed.diagnostics().is_empty());

        let (sources, source_id) = add_source(grouped(512));
        let lexed = lex_parser_source_twice(&sources, source_id, "small-stack expression error");
        assert_parser_error_twice(
            &sources,
            &lexed,
            ParserInternalError::NestingLimitExceeded {
                limit: RECURSION_LIMIT,
            },
            "small-stack expression error",
            parse_expression,
        );
    });

    run_on_small_caller("declaration", || {
        let declaration =
            |depth| format!("val x: {}T{} = 1", "A<".repeat(depth), ">".repeat(depth));
        let (sources, source_id) = add_source(declaration(1_023));
        let parsed =
            parse_declaration_twice(&sources, source_id, "small-stack declaration success");
        assert!(parsed.diagnostics().is_empty());

        let (sources, source_id) = add_source(declaration(1_024));
        let lexed = lex_parser_source_twice(&sources, source_id, "small-stack declaration error");
        assert_parser_error_twice(
            &sources,
            &lexed,
            ParserInternalError::NestingLimitExceeded {
                limit: RECURSION_LIMIT,
            },
            "small-stack declaration error",
            parse_declaration,
        );
    });

    run_on_small_caller("block", || {
        let block = |depth| format!("{}{}", "{".repeat(depth), "}".repeat(depth));
        let (sources, source_id) = add_source(block(1_024));
        let parsed = parse_block_twice(&sources, source_id, "small-stack block success");
        assert!(parsed.diagnostics().is_empty());

        let (sources, source_id) = add_source(block(1_025));
        let lexed = lex_parser_source_twice(&sources, source_id, "small-stack block error");
        assert_parser_error_twice(
            &sources,
            &lexed,
            ParserInternalError::NestingLimitExceeded {
                limit: RECURSION_LIMIT,
            },
            "small-stack block error",
            parse_block,
        );
    });

    run_on_small_caller("file", || {
        let file = |depth| format!("fun boundary() {}{}", "{".repeat(depth), "}".repeat(depth));
        let (sources, source_id) = add_source(file(1_024));
        let parsed = parse_file_twice(&sources, source_id, "small-stack file success");
        assert!(parsed.diagnostics().is_empty());

        let (sources, source_id) = add_source(file(1_025));
        let lexed = lex_parser_source_twice(&sources, source_id, "small-stack file error");
        assert_parser_error_twice(
            &sources,
            &lexed,
            ParserInternalError::NestingLimitExceeded {
                limit: RECURSION_LIMIT,
            },
            "small-stack file error",
            parse_file,
        );
    });
}
