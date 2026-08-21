//! SPEC-0161 的四公开 Parser 入口超长词法错误桥接矩阵。

use lang_frontend::{
    ast::{ExpressionId, ItemId},
    diagnostic::Diagnostic,
    parser::{Expression, ExpressionAst, Item, Statement, StringPart},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    parse_block_twice, parse_declaration_twice, parse_expression_twice, parse_file_twice,
};

const LONG_RUN: usize = 65_536;

#[derive(Clone, Copy)]
enum ExpectedExpression {
    Error,
    String { error_parts: usize },
}

struct LongErrorCase {
    name: &'static str,
    text: String,
    code: &'static str,
    error_start: usize,
    error_end: usize,
    terminal: bool,
    expression: ExpectedExpression,
}

fn long_error_cases() -> Vec<LongErrorCase> {
    let long_text = "a".repeat(LONG_RUN);

    let unterminated_comment = format!("/*{long_text}");
    let unterminated_comment_len = unterminated_comment.len();

    let unterminated_string = format!("\"{long_text}");
    let unterminated_string_len = unterminated_string.len();

    let unterminated_interpolation = format!("\"${{{long_text}");
    let unterminated_interpolation_len = unterminated_interpolation.len();

    let terminal_escape = format!("\"{long_text}\\");
    let terminal_escape_start = terminal_escape.len() - 1;
    let terminal_escape_len = terminal_escape.len();

    let long_suffix = "b".repeat(LONG_RUN);
    let interior_escape = format!("\"{long_text}\\q{long_suffix}\"");
    let interior_escape_start = 1 + LONG_RUN;

    let invalid_char = format!("'{long_text}'");
    let invalid_char_len = invalid_char.len();

    let invalid_number = format!("{}e3", "1".repeat(LONG_RUN));
    let invalid_number_len = invalid_number.len();

    vec![
        LongErrorCase {
            name: "unterminated block comment",
            text: unterminated_comment,
            code: "L0003",
            error_start: 0,
            error_end: unterminated_comment_len,
            terminal: true,
            expression: ExpectedExpression::Error,
        },
        LongErrorCase {
            name: "unterminated string",
            text: unterminated_string,
            code: "L0004",
            error_start: 0,
            error_end: unterminated_string_len,
            terminal: true,
            expression: ExpectedExpression::String { error_parts: 0 },
        },
        LongErrorCase {
            name: "unterminated interpolation",
            text: unterminated_interpolation,
            code: "L0005",
            error_start: 1,
            error_end: unterminated_interpolation_len,
            terminal: true,
            expression: ExpectedExpression::String { error_parts: 0 },
        },
        LongErrorCase {
            name: "terminal escape",
            text: terminal_escape,
            code: "L0006",
            error_start: terminal_escape_start,
            error_end: terminal_escape_len,
            terminal: true,
            expression: ExpectedExpression::String { error_parts: 1 },
        },
        LongErrorCase {
            name: "interior invalid escape",
            text: interior_escape,
            code: "L0006",
            error_start: interior_escape_start,
            error_end: interior_escape_start + 2,
            terminal: false,
            expression: ExpectedExpression::String { error_parts: 1 },
        },
        LongErrorCase {
            name: "closed invalid char",
            text: invalid_char,
            code: "L0007",
            error_start: 0,
            error_end: invalid_char_len,
            terminal: false,
            expression: ExpectedExpression::Error,
        },
        LongErrorCase {
            name: "invalid number",
            text: invalid_number,
            code: "L0008",
            error_start: 0,
            error_end: invalid_number_len,
            terminal: false,
            expression: ExpectedExpression::Error,
        },
    ]
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-long-lexical-error-bridge.ko", source)
        .expect("long lexical error bridge source name must be unique");
    (sources, source_id)
}

fn assert_single_diagnostic(
    diagnostics: &[Diagnostic],
    case: &LongErrorCase,
    payload_start: usize,
    context: &str,
) {
    assert_eq!(
        diagnostics.len(),
        1,
        "unexpected diagnostic count for {context}: {:?}",
        diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ))
            .collect::<Vec<_>>()
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code().to_string(), case.code, "{context}");
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (
            payload_start + case.error_start,
            payload_start + case.error_end
        ),
        "{context}"
    );
}

fn assert_expression(
    ast: &ExpressionAst,
    expression: ExpressionId,
    expected: ExpectedExpression,
    payload_start: usize,
    payload_end: usize,
    context: &str,
) {
    let node = ast
        .expressions()
        .get(expression)
        .unwrap_or_else(|error| panic!("expression lookup failed for {context}: {error}"));
    assert_eq!(
        (node.span().start(), node.span().end()),
        (payload_start, payload_end),
        "{context}"
    );

    match (expected, node.payload()) {
        (ExpectedExpression::Error, Expression::Error) => {}
        (ExpectedExpression::String { error_parts }, Expression::String { parts }) => {
            assert_eq!(
                parts
                    .iter()
                    .filter(|part| matches!(part, StringPart::Error(_)))
                    .count(),
                error_parts,
                "{context}"
            );
        }
        _ => panic!(
            "unexpected expression payload for {context}: {:?}",
            node.payload()
        ),
    }
}

fn variable_initializer(ast: &ExpressionAst, declaration: ItemId, context: &str) -> ExpressionId {
    let Item::Variable { initializer, .. } = ast
        .items()
        .get(declaration)
        .unwrap_or_else(|error| panic!("variable lookup failed for {context}: {error}"))
        .payload()
    else {
        panic!("expected variable declaration for {context}")
    };
    *initializer
}

#[test]
fn every_public_entry_preserves_each_long_lexical_error() {
    let cases = long_error_cases();
    assert_eq!(cases.len(), 7);
    let mut source_count = 0;

    for case in &cases {
        let context = format!("{} expression", case.name);
        let (sources, source_id) = add_source(case.text.clone());
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert_single_diagnostic(parsed.diagnostics(), case, 0, &context);
        assert_expression(
            parsed.ast(),
            parsed.root(),
            case.expression,
            0,
            case.text.len(),
            &context,
        );
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} declaration", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert_single_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        assert_expression(
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.root(), &context),
            case.expression,
            prefix.len(),
            prefix.len() + case.text.len(),
            &context,
        );
        source_count += 1;

        let prefix = "{ val result = ";
        let suffix = if case.terminal { "" } else { " }" };
        let context = format!("{} block", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}{suffix}", case.text));
        let parsed = parse_block_twice(&sources, source_id, &context);
        assert_single_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        let Statement::Block { elements } = parsed
            .ast()
            .statements()
            .get(parsed.root())
            .unwrap_or_else(|error| panic!("block lookup failed for {context}: {error}"))
            .payload()
        else {
            panic!("expected block root for {context}")
        };
        assert_eq!(elements.len(), 1, "{context}");
        let Statement::LocalVariable { declaration } = parsed
            .ast()
            .statements()
            .get(elements[0])
            .unwrap_or_else(|error| panic!("local lookup failed for {context}: {error}"))
            .payload()
        else {
            panic!("expected local variable for {context}")
        };
        assert_expression(
            parsed.ast(),
            variable_initializer(parsed.ast(), *declaration, &context),
            case.expression,
            prefix.len(),
            prefix.len() + case.text.len(),
            &context,
        );
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} file", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_file_twice(&sources, source_id, &context);
        assert_single_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        assert_eq!(parsed.roots().len(), 1, "{context}");
        assert_expression(
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.roots()[0], &context),
            case.expression,
            prefix.len(),
            prefix.len() + case.text.len(),
            &context,
        );
        source_count += 1;
    }

    assert_eq!(source_count, 28);
}
