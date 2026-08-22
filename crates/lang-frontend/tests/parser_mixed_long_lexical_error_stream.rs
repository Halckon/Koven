//! SPEC-0162 的四公开 Parser 入口混合超长词法错误流矩阵。

use lang_frontend::{
    ast::{ExpressionId, ItemId},
    diagnostic::{Diagnostic, DiagnosticDetail},
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

struct ExpectedDiagnostic {
    code: &'static str,
    start: usize,
    end: usize,
}

struct ExpectedArgument {
    start: usize,
    end: usize,
    expression: ExpectedExpression,
}

struct MixedErrorCase {
    name: &'static str,
    text: String,
    diagnostics: Vec<ExpectedDiagnostic>,
    arguments: Vec<ExpectedArgument>,
}

struct TerminalOwner {
    name: &'static str,
    text: String,
    code: &'static str,
    error_start: usize,
    error_end: usize,
    expression: ExpectedExpression,
}

fn terminal_owners() -> Vec<TerminalOwner> {
    let comment = format!("/*{}", "c".repeat(LONG_RUN));
    let comment_len = comment.len();

    let string = format!("\"{}", "d".repeat(LONG_RUN));
    let string_len = string.len();

    let interpolation = format!("\"${{{}", "x".repeat(LONG_RUN));
    let interpolation_len = interpolation.len();

    let escape = format!("\"{}\\", "e".repeat(LONG_RUN));
    let escape_start = escape.len() - 1;
    let escape_len = escape.len();

    vec![
        TerminalOwner {
            name: "unterminated block comment",
            text: comment,
            code: "L0003",
            error_start: 0,
            error_end: comment_len,
            expression: ExpectedExpression::Error,
        },
        TerminalOwner {
            name: "unterminated string",
            text: string,
            code: "L0004",
            error_start: 0,
            error_end: string_len,
            expression: ExpectedExpression::String { error_parts: 0 },
        },
        TerminalOwner {
            name: "unterminated interpolation",
            text: interpolation,
            code: "L0005",
            error_start: 1,
            error_end: interpolation_len,
            expression: ExpectedExpression::String { error_parts: 0 },
        },
        TerminalOwner {
            name: "terminal escape",
            text: escape,
            code: "L0006",
            error_start: escape_start,
            error_end: escape_len,
            expression: ExpectedExpression::String { error_parts: 1 },
        },
    ]
}

fn push_argument(
    text: &mut String,
    argument: &str,
    expression: ExpectedExpression,
    arguments: &mut Vec<ExpectedArgument>,
) -> (usize, usize) {
    if !arguments.is_empty() {
        text.push_str(", ");
    }
    let start = text.len();
    text.push_str(argument);
    let end = text.len();
    arguments.push(ExpectedArgument {
        start,
        end,
        expression,
    });
    (start, end)
}

fn mixed_error_cases() -> Vec<MixedErrorCase> {
    let interior_escape = format!("\"{}\\qz\"", "a".repeat(LONG_RUN));
    let invalid_char = format!("'{}'", "b".repeat(LONG_RUN));
    let invalid_number = format!("{}e3", "1".repeat(LONG_RUN));

    terminal_owners()
        .into_iter()
        .map(|terminal| {
            let mut text = "call(".to_owned();
            let mut diagnostics = Vec::with_capacity(5);
            let mut arguments = Vec::with_capacity(4);

            let (start, _) = push_argument(
                &mut text,
                &interior_escape,
                ExpectedExpression::String { error_parts: 1 },
                &mut arguments,
            );
            diagnostics.push(ExpectedDiagnostic {
                code: "L0006",
                start: start + 1 + LONG_RUN,
                end: start + 1 + LONG_RUN + 2,
            });

            let (start, end) = push_argument(
                &mut text,
                &invalid_char,
                ExpectedExpression::Error,
                &mut arguments,
            );
            diagnostics.push(ExpectedDiagnostic {
                code: "L0007",
                start,
                end,
            });

            let (start, end) = push_argument(
                &mut text,
                &invalid_number,
                ExpectedExpression::Error,
                &mut arguments,
            );
            diagnostics.push(ExpectedDiagnostic {
                code: "L0008",
                start,
                end,
            });

            let (start, _) = push_argument(
                &mut text,
                &terminal.text,
                terminal.expression,
                &mut arguments,
            );
            diagnostics.push(ExpectedDiagnostic {
                code: terminal.code,
                start: start + terminal.error_start,
                end: start + terminal.error_end,
            });
            diagnostics.push(ExpectedDiagnostic {
                code: "L0010",
                start: text.len(),
                end: text.len(),
            });

            MixedErrorCase {
                name: terminal.name,
                text,
                diagnostics,
                arguments,
            }
        })
        .collect()
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-mixed-long-lexical-error-stream.ko", source)
        .expect("mixed long lexical error source name must be unique");
    (sources, source_id)
}

fn assert_diagnostics(
    actual: &[Diagnostic],
    expected: &[ExpectedDiagnostic],
    wrapper_offset: usize,
    context: &str,
) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "unexpected diagnostics for {context}: {:?}",
        actual
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ))
            .collect::<Vec<_>>()
    );
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.code().to_string(), expected.code, "{context}");
        assert_eq!(
            (actual.primary_span().start(), actual.primary_span().end()),
            (
                wrapper_offset + expected.start,
                wrapper_offset + expected.end
            ),
            "{context}"
        );
    }
    let closing = actual.last().expect("closing diagnostic");
    let [DiagnosticDetail::Label(opener)] = closing.details() else {
        panic!("L0010 must retain one opener label for {context}")
    };
    assert_eq!(
        (opener.span().start(), opener.span().end()),
        (wrapper_offset + 4, wrapper_offset + 5),
        "{context}"
    );
    assert_eq!(opener.message(), "opening delimiter is here", "{context}");
}

fn assert_expression(
    ast: &ExpressionAst,
    expression: ExpressionId,
    expected: ExpectedExpression,
    start: usize,
    end: usize,
    context: &str,
) {
    let node = ast
        .expressions()
        .get(expression)
        .unwrap_or_else(|error| panic!("expression lookup failed for {context}: {error}"));
    assert_eq!(
        (node.span().start(), node.span().end()),
        (start, end),
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
            "unexpected argument payload for {context}: {:?}",
            node.payload()
        ),
    }
}

fn assert_call(
    ast: &ExpressionAst,
    call: ExpressionId,
    case: &MixedErrorCase,
    wrapper_offset: usize,
    context: &str,
) {
    let node = ast
        .expressions()
        .get(call)
        .unwrap_or_else(|error| panic!("call lookup failed for {context}: {error}"));
    assert_eq!(
        (node.span().start(), node.span().end()),
        (wrapper_offset, wrapper_offset + case.text.len()),
        "{context}"
    );
    let Expression::Call { arguments, .. } = node.payload() else {
        panic!("expected call expression for {context}")
    };
    assert_eq!(arguments.len(), case.arguments.len(), "{context}");
    for (actual, expected) in arguments.iter().zip(&case.arguments) {
        let start = wrapper_offset + expected.start;
        let end = wrapper_offset + expected.end;
        assert_eq!(
            (actual.span.start(), actual.span.end()),
            (start, end),
            "{context}"
        );
        assert_expression(ast, actual.value, expected.expression, start, end, context);
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
fn every_public_entry_preserves_each_mixed_long_lexical_error_stream() {
    let cases = mixed_error_cases();
    assert_eq!(cases.len(), 4);
    let mut source_count = 0;

    for case in &cases {
        let context = format!("{} expression", case.name);
        let (sources, source_id) = add_source(case.text.clone());
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert_diagnostics(parsed.diagnostics(), &case.diagnostics, 0, &context);
        assert_call(parsed.ast(), parsed.root(), case, 0, &context);
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} declaration", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert_diagnostics(
            parsed.diagnostics(),
            &case.diagnostics,
            prefix.len(),
            &context,
        );
        assert_call(
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.root(), &context),
            case,
            prefix.len(),
            &context,
        );
        source_count += 1;

        let prefix = "{ val result = ";
        let context = format!("{} block", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_block_twice(&sources, source_id, &context);
        assert_diagnostics(
            parsed.diagnostics(),
            &case.diagnostics,
            prefix.len(),
            &context,
        );
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
        assert_call(
            parsed.ast(),
            variable_initializer(parsed.ast(), *declaration, &context),
            case,
            prefix.len(),
            &context,
        );
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} file", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_file_twice(&sources, source_id, &context);
        assert_diagnostics(
            parsed.diagnostics(),
            &case.diagnostics,
            prefix.len(),
            &context,
        );
        assert_eq!(parsed.roots().len(), 1, "{context}");
        assert_call(
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.roots()[0], &context),
            case,
            prefix.len(),
            &context,
        );
        source_count += 1;
    }

    assert_eq!(source_count, 16);
}
