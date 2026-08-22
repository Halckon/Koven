//! SPEC-0164 的四公开 Parser 入口超长 UTF-8 string owner 换行恢复矩阵。

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

const UTF8_SCALARS: usize = 21_845;
const UTF8_BYTES: usize = UTF8_SCALARS * 3;
const AFTER_DECLARATION: &str = "val after = 0";

struct OwnerCase {
    name: &'static str,
    text: String,
    syntax_len: usize,
    code: &'static str,
    error_start: usize,
    error_end: usize,
    error_parts: usize,
}

struct CallCase {
    name: &'static str,
    text: String,
    first_argument_start: usize,
    first_argument_end: usize,
    text_start: usize,
    text_end: usize,
    sentinel_start: usize,
    code: &'static str,
    error_start: usize,
    error_end: usize,
    error_parts: usize,
}

fn owner_cases() -> Vec<OwnerCase> {
    let payload = "界".repeat(UTF8_SCALARS);
    assert_eq!(payload.len(), UTF8_BYTES);

    let unterminated = |name, line_break: &str| {
        let text = format!("\"{payload}{line_break}");
        let syntax_len = 1 + payload.len();
        OwnerCase {
            name,
            text,
            syntax_len,
            code: "L0004",
            error_start: 0,
            error_end: syntax_len,
            error_parts: 0,
        }
    };
    let terminal_escape = |name, line_break: &str| {
        let text = format!("\"{payload}\\{line_break}");
        let syntax_len = 1 + payload.len() + 1;
        OwnerCase {
            name,
            text,
            syntax_len,
            code: "L0006",
            error_start: syntax_len - 1,
            error_end: syntax_len,
            error_parts: 1,
        }
    };

    vec![
        unterminated("unterminated string LF", "\n"),
        unterminated("unterminated string CRLF", "\r\n"),
        terminal_escape("terminal escape LF", "\n"),
        terminal_escape("terminal escape CRLF", "\r\n"),
    ]
}

fn call_cases() -> Vec<CallCase> {
    owner_cases()
        .into_iter()
        .map(|owner| {
            let mut text = "call(".to_owned();
            let first_argument_start = text.len();
            text.push_str(&owner.text);
            let first_argument_end = first_argument_start + owner.syntax_len;
            text.push_str(", sentinel)");
            let sentinel_start = first_argument_start + owner.text.len() + 2;
            CallCase {
                name: owner.name,
                text,
                first_argument_start,
                first_argument_end,
                text_start: first_argument_start + 1,
                text_end: first_argument_start + 1 + UTF8_BYTES,
                sentinel_start,
                code: owner.code,
                error_start: first_argument_start + owner.error_start,
                error_end: first_argument_start + owner.error_end,
                error_parts: owner.error_parts,
            }
        })
        .collect()
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-long-utf8-line-recovery.ko", source)
        .expect("long UTF-8 line recovery source name must be unique");
    (sources, source_id)
}

fn assert_diagnostic(
    diagnostics: &[Diagnostic],
    case: &CallCase,
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
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (
            wrapper_offset + case.error_start,
            wrapper_offset + case.error_end
        ),
        "{context}"
    );
    assert!(diagnostic.details().is_empty(), "{context}");
}

fn assert_utf8_text(sources: &SourceMap, span: lang_frontend::source::Span, context: &str) {
    let text = sources
        .slice(span)
        .unwrap_or_else(|error| panic!("UTF-8 text slice failed for {context}: {error}"));
    assert_eq!(text.len(), UTF8_BYTES, "{context}");
    assert_eq!(text.chars().count(), UTF8_SCALARS, "{context}");
    assert!(text.chars().all(|scalar| scalar == '界'), "{context}");
}

fn assert_call(
    sources: &SourceMap,
    ast: &ExpressionAst,
    call: ExpressionId,
    case: &CallCase,
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
    assert_eq!(arguments.len(), 2, "{context}");

    let first_start = wrapper_offset + case.first_argument_start;
    let first_end = wrapper_offset + case.first_argument_end;
    assert_eq!(
        (arguments[0].span.start(), arguments[0].span.end()),
        (first_start, first_end),
        "{context}"
    );
    let string = ast
        .expressions()
        .get(arguments[0].value)
        .unwrap_or_else(|error| panic!("string lookup failed for {context}: {error}"));
    assert_eq!(
        (string.span().start(), string.span().end()),
        (first_start, first_end),
        "{context}"
    );
    let Expression::String { parts } = string.payload() else {
        panic!("expected recovered string for {context}")
    };
    assert_eq!(
        parts
            .iter()
            .filter(|part| matches!(part, StringPart::Error(_)))
            .count(),
        case.error_parts,
        "{context}"
    );
    let text_span = parts
        .iter()
        .find_map(|part| match part {
            StringPart::Text(span) => Some(*span),
            StringPart::Interpolation { .. } | StringPart::Error(_) => None,
        })
        .unwrap_or_else(|| panic!("missing UTF-8 StringText for {context}"));
    assert_eq!(
        (text_span.start(), text_span.end()),
        (
            wrapper_offset + case.text_start,
            wrapper_offset + case.text_end
        ),
        "{context}"
    );
    assert_utf8_text(sources, text_span, context);

    let sentinel_start = wrapper_offset + case.sentinel_start;
    let sentinel_end = sentinel_start + "sentinel".len();
    assert_eq!(
        (arguments[1].span.start(), arguments[1].span.end()),
        (sentinel_start, sentinel_end),
        "{context}"
    );
    let sentinel = ast
        .expressions()
        .get(arguments[1].value)
        .unwrap_or_else(|error| panic!("sentinel lookup failed for {context}: {error}"));
    assert!(matches!(sentinel.payload(), Expression::Name), "{context}");
    assert_eq!(
        sources
            .slice(sentinel.span())
            .unwrap_or_else(|error| panic!("sentinel slice failed for {context}: {error}")),
        "sentinel",
        "{context}"
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
    assert_eq!(
        sources.slice(node.span()).unwrap_or_else(|error| panic!(
            "after declaration slice failed for {context}: {error}"
        )),
        AFTER_DECLARATION,
        "{context}"
    );
}

#[test]
fn every_public_entry_recovers_each_long_utf8_owner_across_lf_and_crlf() {
    let cases = call_cases();
    assert_eq!(cases.len(), 4);
    let mut source_count = 0;

    for case in &cases {
        let context = format!("{} expression", case.name);
        let (sources, source_id) = add_source(case.text.clone());
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert_diagnostic(parsed.diagnostics(), case, 0, &context);
        assert_call(&sources, parsed.ast(), parsed.root(), case, 0, &context);
        source_count += 1;

        let prefix = "val result = ";
        let context = format!("{} declaration", case.name);
        let (sources, source_id) = add_source(format!("{prefix}{}", case.text));
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert_diagnostic(parsed.diagnostics(), case, prefix.len(), &context);
        assert_call(
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
        assert_call(
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
        assert_call(
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
