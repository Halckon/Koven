//! SPEC-0165 的四公开 Parser 入口超长 UTF-8 嵌套换行恢复矩阵。

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    parser::{Expression, ExpressionAst, Item, Statement, StringPart},
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    parse_block_twice, parse_declaration_twice, parse_expression_twice, parse_file_twice,
};

const UTF8_SCALARS: usize = 21_845;
const UTF8_BYTES: usize = UTF8_SCALARS * 3;
const INNER_SENTINEL: &str = "inner_sentinel";
const OUTER_SENTINEL: &str = "outer_sentinel";
const AFTER_DECLARATION: &str = "val after = 0";

struct NestedCase {
    name: &'static str,
    text: String,
    code: &'static str,
    diagnostic_start: usize,
    diagnostic_end: usize,
    outer_string_start: usize,
    outer_string_end: usize,
    outer_head_start: usize,
    outer_head_end: usize,
    interpolation_start: usize,
    interpolation_end: usize,
    inner_call_start: usize,
    inner_call_end: usize,
    inner_string_start: usize,
    inner_string_end: usize,
    inner_text_start: usize,
    inner_text_end: usize,
    inner_error_parts: usize,
    inner_sentinel_start: usize,
    outer_tail_start: usize,
    outer_tail_end: usize,
    outer_sentinel_start: usize,
}

fn nested_case(
    name: &'static str,
    code: &'static str,
    terminal_escape: bool,
    line_break: &str,
) -> NestedCase {
    let payload = "界".repeat(UTF8_SCALARS);
    assert_eq!(payload.len(), UTF8_BYTES);

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
    let inner_string_start = text.len();
    text.push('"');
    let inner_text_start = text.len();
    text.push_str(&payload);
    let inner_text_end = text.len();
    let diagnostic_start = if terminal_escape {
        let start = text.len();
        text.push('\\');
        start
    } else {
        inner_string_start
    };
    let inner_string_end = text.len();
    let diagnostic_end = inner_string_end;
    text.push_str(line_break);

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

    NestedCase {
        name,
        text,
        code,
        diagnostic_start,
        diagnostic_end,
        outer_string_start,
        outer_string_end,
        outer_head_start,
        outer_head_end,
        interpolation_start,
        interpolation_end,
        inner_call_start,
        inner_call_end,
        inner_string_start,
        inner_string_end,
        inner_text_start,
        inner_text_end,
        inner_error_parts: usize::from(terminal_escape),
        inner_sentinel_start,
        outer_tail_start,
        outer_tail_end,
        outer_sentinel_start,
    }
}

fn nested_cases() -> Vec<NestedCase> {
    vec![
        nested_case("unterminated inner string LF", "L0004", false, "\n"),
        nested_case("unterminated inner string CRLF", "L0004", false, "\r\n"),
        nested_case("terminal inner escape LF", "L0006", true, "\n"),
        nested_case("terminal inner escape CRLF", "L0006", true, "\r\n"),
    ]
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-long-utf8-nested-line-recovery.ko", source)
        .expect("nested long UTF-8 line recovery source name must be unique");
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
    diagnostics: &[Diagnostic],
    case: &NestedCase,
    wrapper_offset: usize,
    context: &str,
) {
    assert_eq!(
        diagnostics.len(),
        1,
        "unexpected diagnostics for {context}: {diagnostics:?}"
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code().to_string(), case.code, "{context}");
    assert_span(
        diagnostic.primary_span(),
        wrapper_offset + case.diagnostic_start,
        wrapper_offset + case.diagnostic_end,
        context,
    );
    assert!(diagnostic.details().is_empty(), "{context}");
}

fn assert_name(
    sources: &SourceMap,
    ast: &ExpressionAst,
    expression: ExpressionId,
    start: usize,
    expected: &str,
    context: &str,
) {
    let node = ast
        .expressions()
        .get(expression)
        .unwrap_or_else(|error| panic!("name lookup failed for {context}: {error}"));
    assert!(matches!(node.payload(), Expression::Name), "{context}");
    assert_span(node.span(), start, start + expected.len(), context);
    assert_slice(sources, node.span(), expected, context);
}

fn assert_utf8_text(sources: &SourceMap, span: Span, context: &str) {
    let text = sources
        .slice(span)
        .unwrap_or_else(|error| panic!("UTF-8 text slice failed for {context}: {error}"));
    assert_eq!(text.len(), UTF8_BYTES, "{context}");
    assert_eq!(text.chars().count(), UTF8_SCALARS, "{context}");
    assert!(text.chars().all(|scalar| scalar == '界'), "{context}");
}

fn assert_nested_call(
    sources: &SourceMap,
    ast: &ExpressionAst,
    root: ExpressionId,
    case: &NestedCase,
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
    let Expression::String { parts: outer_parts } = outer_string.payload() else {
        panic!("expected outer string for {context}")
    };
    let [
        StringPart::Text(head),
        StringPart::Interpolation { span, expression },
        StringPart::Text(tail),
    ] = outer_parts.as_slice()
    else {
        panic!("expected head/interpolation/tail outer string for {context}: {outer_parts:?}")
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

    let inner_string_start = wrapper_offset + case.inner_string_start;
    let inner_string_end = wrapper_offset + case.inner_string_end;
    assert_span(
        inner_arguments[0].span,
        inner_string_start,
        inner_string_end,
        context,
    );
    let inner_string = ast
        .expressions()
        .get(inner_arguments[0].value)
        .unwrap_or_else(|error| panic!("inner string lookup failed for {context}: {error}"));
    assert_span(
        inner_string.span(),
        inner_string_start,
        inner_string_end,
        context,
    );
    let Expression::String { parts: inner_parts } = inner_string.payload() else {
        panic!("expected inner string for {context}")
    };
    assert_eq!(
        inner_parts
            .iter()
            .filter(|part| matches!(part, StringPart::Error(_)))
            .count(),
        case.inner_error_parts,
        "{context}"
    );
    let inner_text = inner_parts
        .iter()
        .find_map(|part| match part {
            StringPart::Text(span) => Some(*span),
            StringPart::Interpolation { .. } | StringPart::Error(_) => None,
        })
        .unwrap_or_else(|| panic!("missing inner UTF-8 text for {context}"));
    assert_span(
        inner_text,
        wrapper_offset + case.inner_text_start,
        wrapper_offset + case.inner_text_end,
        context,
    );
    assert_utf8_text(sources, inner_text, context);
    if case.inner_error_parts == 1 {
        let error = inner_parts
            .iter()
            .find_map(|part| match part {
                StringPart::Error(span) => Some(*span),
                StringPart::Text(_) | StringPart::Interpolation { .. } => None,
            })
            .expect("terminal escape must retain one Error part");
        assert_span(
            error,
            wrapper_offset + case.diagnostic_start,
            wrapper_offset + case.diagnostic_end,
            context,
        );
    }

    assert_name(
        sources,
        ast,
        inner_arguments[1].value,
        wrapper_offset + case.inner_sentinel_start,
        INNER_SENTINEL,
        context,
    );
    assert_name(
        sources,
        ast,
        outer_arguments[1].value,
        wrapper_offset + case.outer_sentinel_start,
        OUTER_SENTINEL,
        context,
    );
}

fn variable_initializer(
    ast: &ExpressionAst,
    declaration: lang_frontend::ast::ItemId,
) -> ExpressionId {
    let Item::Variable { initializer, .. } = ast
        .items()
        .get(declaration)
        .expect("variable declaration")
        .payload()
    else {
        panic!("expected variable declaration")
    };
    *initializer
}

fn assert_after_declaration(
    sources: &SourceMap,
    ast: &ExpressionAst,
    declaration: lang_frontend::ast::ItemId,
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
fn every_public_entry_restores_the_complete_mode_stack_after_each_long_nested_line_error() {
    let cases = nested_cases();
    assert_eq!(cases.len(), 4);
    let mut source_count = 0;

    for case in &cases {
        let context = format!("{} expression", case.name);
        let (sources, source_id) = add_source(case.text.clone());
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert_diagnostic(parsed.diagnostics(), case, 0, &context);
        assert_nested_call(&sources, parsed.ast(), parsed.root(), case, 0, &context);
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} declaration", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        assert_nested_call(
            &sources,
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.root()),
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
        assert_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
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
        assert_nested_call(
            &sources,
            parsed.ast(),
            variable_initializer(parsed.ast(), declarations[0]),
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
        assert_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        assert_eq!(parsed.roots().len(), 2, "{context}");
        assert_nested_call(
            &sources,
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.roots()[0]),
            case,
            prefix.len(),
            &context,
        );
        assert_after_declaration(&sources, parsed.ast(), parsed.roots()[1], &context);
        source_count += 1;
    }

    assert_eq!(source_count, 16);
}
