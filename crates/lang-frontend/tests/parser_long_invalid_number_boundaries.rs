//! SPEC-0167 的四公开 Parser 入口超长非法数字 maximal-region / operator-boundary 矩阵。

use lang_frontend::{
    ast::{ExpressionId, ItemId},
    diagnostic::Diagnostic,
    parser::{
        BinaryOperator, Expression, ExpressionAst, IntegerLiteralKind, Item, LiteralKind,
        Statement, StringPart,
    },
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    parse_block_twice, parse_declaration_twice, parse_expression_twice, parse_file_twice,
};

const LONG_RUN: usize = 65_536;
const RHS: &str = "rhs";
const INNER_SENTINEL: &str = "inner_sentinel";
const OUTER_SENTINEL: &str = "outer_sentinel";
const AFTER_DECLARATION: &str = "val after = 0";

struct NumberCase {
    name: &'static str,
    text: String,
    invalid: String,
    invalid_start: usize,
    invalid_end: usize,
    plus_start: usize,
    plus_end: usize,
    rhs_start: usize,
    rhs_end: usize,
    outer_string_start: usize,
    outer_string_end: usize,
    outer_head_start: usize,
    outer_head_end: usize,
    interpolation_start: usize,
    interpolation_end: usize,
    inner_call_start: usize,
    inner_call_end: usize,
    inner_sentinel_start: usize,
    outer_tail_start: usize,
    outer_tail_end: usize,
    outer_sentinel_start: usize,
}

fn number_case(name: &'static str, invalid: String) -> NumberCase {
    assert!(invalid.len() >= LONG_RUN);
    assert!(invalid.is_ascii());

    let mut text = "outer(".to_owned();
    let outer_string_start = text.len();
    text.push('"');
    let outer_head_start = text.len();
    text.push_str("head");
    let outer_head_end = text.len();

    let interpolation_start = text.len();
    text.push_str("${");
    let inner_call_start = text.len();
    text.push_str("inner(");
    let invalid_start = text.len();
    text.push_str(&invalid);
    let invalid_end = text.len();
    text.push(' ');
    let plus_start = text.len();
    text.push('+');
    let plus_end = text.len();
    text.push(' ');
    let rhs_start = text.len();
    text.push_str(RHS);
    let rhs_end = text.len();

    text.push_str(", ");
    let inner_sentinel_start = text.len();
    text.push_str(INNER_SENTINEL);
    text.push(')');
    let inner_call_end = text.len();
    text.push('}');
    let interpolation_end = text.len();

    let outer_tail_start = text.len();
    text.push_str("tail");
    let outer_tail_end = text.len();
    text.push('"');
    let outer_string_end = text.len();
    text.push_str(", ");
    let outer_sentinel_start = text.len();
    text.push_str(OUTER_SENTINEL);
    text.push(')');

    NumberCase {
        name,
        text,
        invalid,
        invalid_start,
        invalid_end,
        plus_start,
        plus_end,
        rhs_start,
        rhs_end,
        outer_string_start,
        outer_string_end,
        outer_head_start,
        outer_head_end,
        interpolation_start,
        interpolation_end,
        inner_call_start,
        inner_call_end,
        inner_sentinel_start,
        outer_tail_start,
        outer_tail_end,
        outer_sentinel_start,
    }
}

fn number_cases() -> Vec<NumberCase> {
    vec![
        number_case(
            "integer exponent tail",
            format!("{}e3", "1".repeat(LONG_RUN)),
        ),
        number_case(
            "fraction exponent tail",
            format!("1.{}e3", "2".repeat(LONG_RUN)),
        ),
        number_case(
            "legal unsigned-long suffix then identifier tail",
            format!("1uL{}", "A".repeat(LONG_RUN)),
        ),
        number_case(
            "unsupported octal radix prefix tail",
            format!("0o{}", "7".repeat(LONG_RUN)),
        ),
        number_case(
            "hexadecimal digits then invalid identifier tail",
            format!("0x{}G", "A".repeat(LONG_RUN)),
        ),
        number_case(
            "binary digits then invalid digit tail",
            format!("0b{}2", "1".repeat(LONG_RUN)),
        ),
    ]
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-long-invalid-number-boundaries.ko", source)
        .expect("long invalid number boundary source name must be unique");
    (sources, source_id)
}

fn assert_span(span: Span, start: usize, end: usize, context: &str) {
    assert_eq!((span.start(), span.end()), (start, end), "{context}");
}

fn assert_slice(sources: &SourceMap, span: Span, expected: &str, context: &str) {
    assert_eq!(
        sources
            .slice(span)
            .unwrap_or_else(|error| panic!("source slice failed for {context}: {error}")),
        expected,
        "{context}"
    );
}

fn assert_diagnostic(
    sources: &SourceMap,
    diagnostics: &[Diagnostic],
    case: &NumberCase,
    wrapper_offset: usize,
    context: &str,
) {
    assert_eq!(
        diagnostics.len(),
        1,
        "unexpected diagnostics for {context}: {diagnostics:?}"
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code().to_string(), "L0008", "{context}");
    assert_span(
        diagnostic.primary_span(),
        wrapper_offset + case.invalid_start,
        wrapper_offset + case.invalid_end,
        context,
    );
    assert_slice(sources, diagnostic.primary_span(), &case.invalid, context);
    assert!(diagnostic.details().is_empty(), "{context}");
}

fn assert_name(
    sources: &SourceMap,
    ast: &ExpressionAst,
    expression: ExpressionId,
    span: Span,
    start: usize,
    expected: &str,
    context: &str,
) {
    assert_span(span, start, start + expected.len(), context);
    let node = ast
        .expressions()
        .get(expression)
        .unwrap_or_else(|error| panic!("name lookup failed for {context}: {error}"));
    assert!(matches!(node.payload(), Expression::Name), "{context}");
    assert_span(node.span(), start, start + expected.len(), context);
    assert_slice(sources, node.span(), expected, context);
}

fn assert_number_expression(
    sources: &SourceMap,
    ast: &ExpressionAst,
    expression: ExpressionId,
    argument_span: Span,
    case: &NumberCase,
    wrapper_offset: usize,
    context: &str,
) {
    let invalid_start = wrapper_offset + case.invalid_start;
    let invalid_end = wrapper_offset + case.invalid_end;
    let rhs_end = wrapper_offset + case.rhs_end;
    assert_span(argument_span, invalid_start, rhs_end, context);

    let binary = ast
        .expressions()
        .get(expression)
        .unwrap_or_else(|error| panic!("binary lookup failed for {context}: {error}"));
    assert_span(binary.span(), invalid_start, rhs_end, context);
    let Expression::Binary {
        left,
        operator,
        operator_span,
        right,
    } = binary.payload()
    else {
        panic!("expected Error + rhs binary for {context}")
    };
    assert_eq!(*operator, BinaryOperator::Add, "{context}");
    assert_span(
        *operator_span,
        wrapper_offset + case.plus_start,
        wrapper_offset + case.plus_end,
        context,
    );
    assert_slice(sources, *operator_span, "+", context);

    let invalid = ast
        .expressions()
        .get(*left)
        .unwrap_or_else(|error| panic!("invalid number lookup failed for {context}: {error}"));
    assert!(matches!(invalid.payload(), Expression::Error), "{context}");
    assert_span(invalid.span(), invalid_start, invalid_end, context);
    assert_slice(sources, invalid.span(), &case.invalid, context);
    assert_eq!(invalid.span().end(), operator_span.start() - 1, "{context}");

    assert_name(
        sources,
        ast,
        *right,
        ast.expressions()
            .get(*right)
            .unwrap_or_else(|error| panic!("rhs lookup failed for {context}: {error}"))
            .span(),
        wrapper_offset + case.rhs_start,
        RHS,
        context,
    );
}

fn assert_recovered_number_call(
    sources: &SourceMap,
    ast: &ExpressionAst,
    root: ExpressionId,
    case: &NumberCase,
    wrapper_offset: usize,
    context: &str,
) {
    let outer_call = ast
        .expressions()
        .get(root)
        .unwrap_or_else(|error| panic!("outer call lookup failed for {context}: {error}"));
    assert_span(
        outer_call.span(),
        wrapper_offset,
        wrapper_offset + case.text.len(),
        context,
    );
    let Expression::Call {
        arguments: outer_arguments,
        ..
    } = outer_call.payload()
    else {
        panic!("expected outer call for {context}")
    };
    assert_eq!(outer_arguments.len(), 2, "{context}");

    let outer_string_start = wrapper_offset + case.outer_string_start;
    let outer_string_end = wrapper_offset + case.outer_string_end;
    assert_span(
        outer_arguments[0].span,
        outer_string_start,
        outer_string_end,
        context,
    );
    let outer_string = ast
        .expressions()
        .get(outer_arguments[0].value)
        .unwrap_or_else(|error| panic!("outer string lookup failed for {context}: {error}"));
    assert_span(
        outer_string.span(),
        outer_string_start,
        outer_string_end,
        context,
    );
    let Expression::String { parts } = outer_string.payload() else {
        panic!("expected outer string for {context}")
    };
    let [
        StringPart::Text(head),
        StringPart::Interpolation { span, expression },
        StringPart::Text(tail),
    ] = parts.as_slice()
    else {
        panic!("expected head/interpolation/tail outer string for {context}: {parts:?}")
    };
    assert_span(
        *head,
        wrapper_offset + case.outer_head_start,
        wrapper_offset + case.outer_head_end,
        context,
    );
    assert_slice(sources, *head, "head", context);
    assert_span(
        *span,
        wrapper_offset + case.interpolation_start,
        wrapper_offset + case.interpolation_end,
        context,
    );
    assert_span(
        *tail,
        wrapper_offset + case.outer_tail_start,
        wrapper_offset + case.outer_tail_end,
        context,
    );
    assert_slice(sources, *tail, "tail", context);

    let inner_call = ast
        .expressions()
        .get(*expression)
        .unwrap_or_else(|error| panic!("inner call lookup failed for {context}: {error}"));
    assert_span(
        inner_call.span(),
        wrapper_offset + case.inner_call_start,
        wrapper_offset + case.inner_call_end,
        context,
    );
    let Expression::Call {
        arguments: inner_arguments,
        ..
    } = inner_call.payload()
    else {
        panic!("expected inner call for {context}")
    };
    assert_eq!(inner_arguments.len(), 2, "{context}");
    assert_number_expression(
        sources,
        ast,
        inner_arguments[0].value,
        inner_arguments[0].span,
        case,
        wrapper_offset,
        context,
    );
    assert_name(
        sources,
        ast,
        inner_arguments[1].value,
        inner_arguments[1].span,
        wrapper_offset + case.inner_sentinel_start,
        INNER_SENTINEL,
        context,
    );
    assert_name(
        sources,
        ast,
        outer_arguments[1].value,
        outer_arguments[1].span,
        wrapper_offset + case.outer_sentinel_start,
        OUTER_SENTINEL,
        context,
    );
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
    assert_slice(sources, node.span(), AFTER_DECLARATION, context);
}

#[test]
fn every_public_entry_preserves_each_long_invalid_number_before_a_real_operator() {
    let cases = number_cases();
    assert_eq!(cases.len(), 6);
    let mut source_count = 0;

    for case in &cases {
        let context = format!("{} expression", case.name);
        let (sources, source_id) = add_source(case.text.clone());
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert_diagnostic(&sources, parsed.diagnostics(), case, 0, &context);
        assert_recovered_number_call(&sources, parsed.ast(), parsed.root(), case, 0, &context);
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} declaration", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert_diagnostic(&sources, parsed.diagnostics(), case, prefix.len(), &context);
        assert_recovered_number_call(
            &sources,
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.root(), &context),
            case,
            prefix.len(),
            &context,
        );
        source_count += 1;

        let prefix = "{ val result = ";
        let suffix = format!("\n{AFTER_DECLARATION} }}");
        let context = format!("{} block", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}{suffix}", case.text));
        let parsed = parse_block_twice(&sources, source_id, &context);
        assert_diagnostic(&sources, parsed.diagnostics(), case, prefix.len(), &context);
        let Statement::Block { elements } = parsed
            .ast()
            .statements()
            .get(parsed.root())
            .unwrap_or_else(|error| panic!("block lookup failed for {context}: {error}"))
            .payload()
        else {
            panic!("expected block root for {context}")
        };
        assert_eq!(elements.len(), 2, "{context}");
        let declarations = elements
            .iter()
            .map(|element| {
                let Statement::LocalVariable { declaration } = parsed
                    .ast()
                    .statements()
                    .get(*element)
                    .unwrap_or_else(|error| panic!("local lookup failed for {context}: {error}"))
                    .payload()
                else {
                    panic!("expected local variable for {context}")
                };
                *declaration
            })
            .collect::<Vec<_>>();
        assert_recovered_number_call(
            &sources,
            parsed.ast(),
            variable_initializer(parsed.ast(), declarations[0], &context),
            case,
            prefix.len(),
            &context,
        );
        assert_after_declaration(&sources, parsed.ast(), declarations[1], &context);
        source_count += 1;

        let prefix = "val result = ";
        let suffix = format!("\n{AFTER_DECLARATION}");
        let context = format!("{} file", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}{suffix}", case.text));
        let parsed = parse_file_twice(&sources, source_id, &context);
        assert_diagnostic(&sources, parsed.diagnostics(), case, prefix.len(), &context);
        assert_eq!(parsed.roots().len(), 2, "{context}");
        assert_recovered_number_call(
            &sources,
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.roots()[0], &context),
            case,
            prefix.len(),
            &context,
        );
        assert_after_declaration(&sources, parsed.ast(), parsed.roots()[1], &context);
        source_count += 1;
    }

    assert_eq!(source_count, 24);
}

#[test]
fn long_supported_radix_numbers_remain_literals_before_a_real_operator() {
    // Guide 01 接受 hex/bin；数值溢出由后续阶段判断，不是 Lexer/Parser 错误。
    for (prefix, digit) in [("0x", "A"), ("0X", "A"), ("0b", "1"), ("0B", "1")] {
        let literal = format!("{prefix}{}", digit.repeat(LONG_RUN));
        let text = format!("{literal} + {RHS}");
        let context = format!("long supported {prefix} literal");
        let (sources, source_id) = add_source(text.clone());
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert!(parsed.diagnostics().is_empty(), "{context}");

        let binary = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("supported radix expression root must resolve");
        assert_span(binary.span(), 0, text.len(), &context);
        let Expression::Binary {
            left,
            operator,
            operator_span,
            right,
        } = binary.payload()
        else {
            panic!("expected integer + rhs binary for {context}")
        };
        assert_eq!(*operator, BinaryOperator::Add, "{context}");
        assert_span(
            *operator_span,
            literal.len() + 1,
            literal.len() + 2,
            &context,
        );
        assert_slice(&sources, *operator_span, "+", &context);

        let number = parsed
            .ast()
            .expressions()
            .get(*left)
            .expect("supported radix literal must resolve");
        assert_eq!(
            number.payload(),
            &Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed)),
            "{context}"
        );
        assert_span(number.span(), 0, literal.len(), &context);
        assert_slice(&sources, number.span(), &literal, &context);
        assert_name(
            &sources,
            parsed.ast(),
            *right,
            parsed
                .ast()
                .expressions()
                .get(*right)
                .expect("rhs of supported radix literal must resolve")
                .span(),
            literal.len() + 3,
            RHS,
            &context,
        );
    }
}
