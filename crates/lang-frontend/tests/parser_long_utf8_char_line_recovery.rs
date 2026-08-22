//! SPEC-0166 的四公开 Parser 入口超长 UTF-8 invalid Char 换行恢复矩阵。

use lang_frontend::{
    ast::{ExpressionId, ItemId},
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

struct CharCase {
    name: &'static str,
    text: String,
    has_terminal_backslash: bool,
    invalid_start: usize,
    invalid_end: usize,
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

fn char_case(name: &'static str, has_terminal_backslash: bool, line_break: &str) -> CharCase {
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
    let invalid_start = text.len();
    text.push('\'');
    text.push_str(&payload);
    if has_terminal_backslash {
        text.push('\\');
    }
    let invalid_end = text.len();
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

    CharCase {
        name,
        text,
        has_terminal_backslash,
        invalid_start,
        invalid_end,
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

fn char_cases() -> Vec<CharCase> {
    vec![
        char_case("direct LF", false, "\n"),
        char_case("direct CRLF", false, "\r\n"),
        char_case("terminal backslash LF", true, "\n"),
        char_case("terminal backslash CRLF", true, "\r\n"),
    ]
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-long-utf8-char-line-recovery.ko", source)
        .expect("long UTF-8 Char line recovery source name must be unique");
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
    case: &CharCase,
    wrapper_offset: usize,
    context: &str,
) {
    assert_eq!(
        diagnostics.len(),
        1,
        "unexpected diagnostics for {context}: {diagnostics:?}"
    );
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code().to_string(), "L0007", "{context}");
    assert_span(
        diagnostic.primary_span(),
        wrapper_offset + case.invalid_start,
        wrapper_offset + case.invalid_end,
        context,
    );
    assert!(diagnostic.details().is_empty(), "{context}");
}

fn assert_name(
    sources: &SourceMap,
    ast: &ExpressionAst,
    expression: ExpressionId,
    argument_span: Span,
    start: usize,
    expected: &str,
    context: &str,
) {
    assert_span(argument_span, start, start + expected.len(), context);
    let node = ast
        .expressions()
        .get(expression)
        .unwrap_or_else(|error| panic!("name lookup failed for {context}: {error}"));
    assert!(matches!(node.payload(), Expression::Name), "{context}");
    assert_span(node.span(), start, start + expected.len(), context);
    assert_slice(sources, node.span(), expected, context);
}

fn assert_invalid_char(sources: &SourceMap, span: Span, case: &CharCase, context: &str) {
    let text = sources
        .slice(span)
        .unwrap_or_else(|error| panic!("invalid Char slice failed for {context}: {error}"));
    assert_eq!(
        text.len(),
        1 + UTF8_BYTES + usize::from(case.has_terminal_backslash),
        "{context}"
    );
    assert!(text.starts_with('\''), "{context}");
    let payload = &text[1..1 + UTF8_BYTES];
    assert_eq!(payload.chars().count(), UTF8_SCALARS, "{context}");
    assert!(payload.chars().all(|scalar| scalar == '界'), "{context}");
    assert_eq!(
        text.ends_with('\\'),
        case.has_terminal_backslash,
        "{context}"
    );
}

fn assert_recovered_char_call(
    sources: &SourceMap,
    ast: &ExpressionAst,
    root: ExpressionId,
    case: &CharCase,
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

    let invalid_start = wrapper_offset + case.invalid_start;
    let invalid_end = wrapper_offset + case.invalid_end;
    assert_span(inner_arguments[0].span, invalid_start, invalid_end, context);
    let invalid_char = ast
        .expressions()
        .get(inner_arguments[0].value)
        .unwrap_or_else(|error| panic!("invalid Char lookup failed for {context}: {error}"));
    assert!(
        matches!(invalid_char.payload(), Expression::Error),
        "{context}"
    );
    assert_span(invalid_char.span(), invalid_start, invalid_end, context);
    assert_invalid_char(sources, invalid_char.span(), case, context);

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
fn every_public_entry_recovers_each_long_utf8_char_before_lf_and_crlf() {
    let cases = char_cases();
    assert_eq!(cases.len(), 4);
    let mut source_count = 0;

    for case in &cases {
        let context = format!("{} expression", case.name);
        let (sources, source_id) = add_source(case.text.clone());
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert_diagnostic(parsed.diagnostics(), case, 0, &context);
        assert_recovered_char_call(&sources, parsed.ast(), parsed.root(), case, 0, &context);
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} declaration", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        assert_recovered_char_call(
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
        assert_recovered_char_call(
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
        assert_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        assert_eq!(parsed.roots().len(), 2, "{context}");
        assert_recovered_char_call(
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

    assert_eq!(source_count, 16);
}
