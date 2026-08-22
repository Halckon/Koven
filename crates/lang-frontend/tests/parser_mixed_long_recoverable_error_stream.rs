//! SPEC-0163 的四公开 Parser 入口混合超长可恢复词法错误流矩阵。

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
const AFTER_DECLARATION: &str = "val after = 0";

#[derive(Clone, Copy)]
enum ExpectedExpression {
    Error,
    Name,
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

struct RecoverableErrorCase {
    text: String,
    diagnostics: Vec<ExpectedDiagnostic>,
    arguments: Vec<ExpectedArgument>,
}

fn push_argument(
    text: &mut String,
    source: &str,
    syntax_len: usize,
    expression: ExpectedExpression,
    arguments: &mut Vec<ExpectedArgument>,
) -> usize {
    if !arguments.is_empty() {
        text.push_str(", ");
    }
    let start = text.len();
    text.push_str(source);
    arguments.push(ExpectedArgument {
        start,
        end: start + syntax_len,
        expression,
    });
    start
}

fn recoverable_error_case() -> RecoverableErrorCase {
    let closed_escape = format!("\"{}\\qz\"", "a".repeat(LONG_RUN));
    let newline_string = format!("\"{}\n", "b".repeat(LONG_RUN));
    let newline_escape = format!("\"{}\\\n", "c".repeat(LONG_RUN));
    let invalid_char = format!("'{}'", "d".repeat(LONG_RUN));
    let invalid_number = format!("{}e3", "1".repeat(LONG_RUN));

    let mut text = "call(".to_owned();
    let mut diagnostics = Vec::with_capacity(5);
    let mut arguments = Vec::with_capacity(6);

    let start = push_argument(
        &mut text,
        &closed_escape,
        closed_escape.len(),
        ExpectedExpression::String { error_parts: 1 },
        &mut arguments,
    );
    diagnostics.push(ExpectedDiagnostic {
        code: "L0006",
        start: start + 1 + LONG_RUN,
        end: start + 1 + LONG_RUN + 2,
    });

    let newline_string_syntax_len = newline_string.len() - 1;
    let start = push_argument(
        &mut text,
        &newline_string,
        newline_string_syntax_len,
        ExpectedExpression::String { error_parts: 0 },
        &mut arguments,
    );
    diagnostics.push(ExpectedDiagnostic {
        code: "L0004",
        start,
        end: start + newline_string_syntax_len,
    });

    let newline_escape_syntax_len = newline_escape.len() - 1;
    let start = push_argument(
        &mut text,
        &newline_escape,
        newline_escape_syntax_len,
        ExpectedExpression::String { error_parts: 1 },
        &mut arguments,
    );
    diagnostics.push(ExpectedDiagnostic {
        code: "L0006",
        start: start + newline_escape_syntax_len - 1,
        end: start + newline_escape_syntax_len,
    });

    let start = push_argument(
        &mut text,
        &invalid_char,
        invalid_char.len(),
        ExpectedExpression::Error,
        &mut arguments,
    );
    diagnostics.push(ExpectedDiagnostic {
        code: "L0007",
        start,
        end: start + invalid_char.len(),
    });

    let start = push_argument(
        &mut text,
        &invalid_number,
        invalid_number.len(),
        ExpectedExpression::Error,
        &mut arguments,
    );
    diagnostics.push(ExpectedDiagnostic {
        code: "L0008",
        start,
        end: start + invalid_number.len(),
    });

    push_argument(
        &mut text,
        "sentinel",
        "sentinel".len(),
        ExpectedExpression::Name,
        &mut arguments,
    );
    text.push(')');

    RecoverableErrorCase {
        text,
        diagnostics,
        arguments,
    }
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-mixed-long-recoverable-error-stream.ko", source)
        .expect("mixed long recoverable error source name must be unique");
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
        assert!(actual.details().is_empty(), "{context}: {actual:?}");
    }
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
        (ExpectedExpression::Error, Expression::Error)
        | (ExpectedExpression::Name, Expression::Name) => {}
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
    case: &RecoverableErrorCase,
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

fn assert_after_declaration(
    sources: &SourceMap,
    ast: &ExpressionAst,
    declaration: ItemId,
    context: &str,
) {
    let node = ast
        .items()
        .get(declaration)
        .unwrap_or_else(|error| panic!("after declaration lookup failed for {context}: {error}"));
    assert!(matches!(node.payload(), Item::Variable { .. }), "{context}");
    assert_eq!(
        sources.slice(node.span()).unwrap_or_else(|error| panic!(
            "after declaration slice failed for {context}: {error}"
        )),
        AFTER_DECLARATION,
        "{context}"
    );
}

#[test]
fn every_public_entry_resumes_after_the_mixed_long_recoverable_error_stream() {
    let case = recoverable_error_case();
    assert_eq!(case.arguments.len(), 6);
    assert_eq!(case.diagnostics.len(), 5);
    let mut source_count = 0;

    let context = "expression";
    let (sources, source_id) = add_source(case.text.clone());
    let parsed = parse_expression_twice(&sources, source_id, context);
    assert_diagnostics(parsed.diagnostics(), &case.diagnostics, 0, context);
    assert_call(parsed.ast(), parsed.root(), &case, 0, context);
    source_count += 1;

    let prefix = "val result = ";
    let context = "declaration";
    let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
    let parsed = parse_declaration_twice(&sources, source_id, context);
    assert_diagnostics(
        parsed.diagnostics(),
        &case.diagnostics,
        prefix.len(),
        context,
    );
    assert_call(
        parsed.ast(),
        variable_initializer(parsed.ast(), parsed.root(), context),
        &case,
        prefix.len(),
        context,
    );
    source_count += 1;

    let prefix = "{ val result = ";
    let suffix = format!("\n{AFTER_DECLARATION} }}");
    let context = "block";
    let (sources, source_id) = add_source(format!("{prefix}{}{suffix}", case.text));
    let parsed = parse_block_twice(&sources, source_id, context);
    assert_diagnostics(
        parsed.diagnostics(),
        &case.diagnostics,
        prefix.len(),
        context,
    );
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(parsed.root())
        .expect("block root")
        .payload()
    else {
        panic!("expected block root")
    };
    assert_eq!(elements.len(), 2);
    let mut declarations = elements.iter().map(|element| {
        let Statement::LocalVariable { declaration } = parsed
            .ast()
            .statements()
            .get(*element)
            .expect("block local")
            .payload()
        else {
            panic!("expected block local")
        };
        *declaration
    });
    let result = declarations.next().expect("result local");
    let after = declarations.next().expect("after local");
    assert_call(
        parsed.ast(),
        variable_initializer(parsed.ast(), result, context),
        &case,
        prefix.len(),
        context,
    );
    assert_after_declaration(&sources, parsed.ast(), after, context);
    source_count += 1;

    let prefix = "val result = ";
    let suffix = format!("\n{AFTER_DECLARATION}");
    let context = "file";
    let (sources, source_id) = add_source(format!("{prefix}{}{suffix}", case.text));
    let parsed = parse_file_twice(&sources, source_id, context);
    assert_diagnostics(
        parsed.diagnostics(),
        &case.diagnostics,
        prefix.len(),
        context,
    );
    assert_eq!(parsed.roots().len(), 2);
    assert_call(
        parsed.ast(),
        variable_initializer(parsed.ast(), parsed.roots()[0], context),
        &case,
        prefix.len(),
        context,
    );
    assert_after_declaration(&sources, parsed.ast(), parsed.roots()[1], context);
    source_count += 1;

    assert_eq!(source_count, 4);
}
