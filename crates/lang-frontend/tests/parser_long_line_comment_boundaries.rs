//! SPEC-0169 的超长 UTF-8 line comment / newline trivia 与四入口语法矩阵。

use lang_frontend::{
    ast::{ExpressionId, ItemId},
    lexer::{LexedFile, Lexeme, LexemeKind, TriviaKind},
    parser::{BinaryOperator, Expression, ExpressionAst, Item, Statement},
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    lex_parser_source_twice, parse_block_twice, parse_declaration_twice, parse_expression_twice,
    parse_file_twice,
};

const UTF8_SCALARS: usize = 21_845;
const UTF8_BYTES: usize = UTF8_SCALARS * 3;
const FIRST_DECLARATION: &str = "val first = 0";
const AFTER_DECLARATION: &str = "val after = 1";

struct Carrier {
    name: &'static str,
    comment: String,
    payload_start: usize,
    payload_end: usize,
    line_break: &'static str,
}

fn carrier(name: &'static str, line_break: &'static str) -> Carrier {
    let payload = "界".repeat(UTF8_SCALARS);
    assert_eq!(payload.len(), UTF8_BYTES);

    let mut comment = "// block-like /* */ string-like \"${ignored}\" marker ".to_owned();
    let payload_start = comment.len();
    comment.push_str(&payload);
    let payload_end = comment.len();
    Carrier {
        name,
        comment,
        payload_start,
        payload_end,
        line_break,
    }
}

fn carriers() -> Vec<Carrier> {
    vec![carrier("LF", "\n"), carrier("CRLF", "\r\n")]
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-long-line-comment-boundaries.ko", source)
        .expect("long line comment source name must be unique");
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

fn only_trivia<'a>(lexed: &'a LexedFile, kind: TriviaKind, context: &str) -> &'a Lexeme {
    let matches = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Trivia(actual) if actual == kind))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "{context}: {kind:?}");
    matches[0]
}

fn assert_comment_lexemes(
    sources: &SourceMap,
    lexed: &LexedFile,
    carrier: &Carrier,
    comment_start: usize,
    context: &str,
) {
    assert!(lexed.diagnostics().is_empty(), "{context}");
    let comment = only_trivia(lexed, TriviaKind::LineComment, context);
    let newline = only_trivia(lexed, TriviaKind::Newline, context);
    let comment_end = comment_start + carrier.comment.len();
    assert_span(comment.span(), comment_start, comment_end, context);
    assert_span(
        newline.span(),
        comment_end,
        comment_end + carrier.line_break.len(),
        context,
    );
    assert_eq!(comment.span().end(), newline.span().start(), "{context}");
    assert_slice(sources, comment.span(), &carrier.comment, context);
    assert_slice(sources, newline.span(), carrier.line_break, context);

    let payload_span = sources
        .span(
            comment.span().source_id(),
            comment_start + carrier.payload_start,
            comment_start + carrier.payload_end,
        )
        .expect("payload Span must be source-valid");
    let payload = sources
        .slice(payload_span)
        .unwrap_or_else(|error| panic!("payload slice failed for {context}: {error}"));
    assert_eq!(payload.len(), UTF8_BYTES, "{context}");
    assert_eq!(payload.chars().count(), UTF8_SCALARS, "{context}");
    assert!(payload.chars().all(|scalar| scalar == '界'), "{context}");
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

fn assert_add_expression(
    sources: &SourceMap,
    ast: &ExpressionAst,
    expression: ExpressionId,
    expression_start: usize,
    carrier: &Carrier,
    context: &str,
) {
    let comment_start = expression_start + "left ".len();
    let plus_start = comment_start + carrier.comment.len() + carrier.line_break.len();
    let right_start = plus_start + "+ ".len();
    let expression_end = right_start + "right".len();
    let node = ast
        .expressions()
        .get(expression)
        .unwrap_or_else(|error| panic!("binary lookup failed for {context}: {error}"));
    assert_span(node.span(), expression_start, expression_end, context);
    let Expression::Binary {
        left,
        operator,
        operator_span,
        right,
    } = node.payload()
    else {
        panic!("expected left + right for {context}")
    };
    assert_eq!(*operator, BinaryOperator::Add, "{context}");
    assert_span(*operator_span, plus_start, plus_start + 1, context);
    assert_slice(sources, *operator_span, "+", context);
    assert_name(sources, ast, *left, expression_start, "left", context);
    assert_name(sources, ast, *right, right_start, "right", context);
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

fn assert_declaration(
    sources: &SourceMap,
    ast: &ExpressionAst,
    declaration: ItemId,
    expected: &str,
    context: &str,
) {
    let node = ast
        .items()
        .get(declaration)
        .unwrap_or_else(|error| panic!("declaration lookup failed for {context}: {error}"));
    assert!(matches!(node.payload(), Item::Variable { .. }), "{context}");
    assert_slice(sources, node.span(), expected, context);
}

#[test]
fn every_public_entry_preserves_long_line_comment_and_newline_boundaries() {
    let carriers = carriers();
    assert_eq!(carriers.len(), 2);
    let mut source_count = 0;

    for carrier in &carriers {
        let expression = format!("left {}{}+ right", carrier.comment, carrier.line_break);
        let comment_start = "left ".len();
        let context = format!("{} expression", carrier.name);
        let (sources, source_id) = add_source(expression);
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexemes(&sources, &lexed, carrier, comment_start, &context);
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert!(parsed.diagnostics().is_empty(), "{context}");
        assert_add_expression(&sources, parsed.ast(), parsed.root(), 0, carrier, &context);
        source_count += 1;

        let declaration_prefix = "val result = ";
        let expression = format!("left {}{}+ right", carrier.comment, carrier.line_break);
        let context = format!("{} declaration", carrier.name);
        let (sources, source_id) = add_source(format!("{declaration_prefix}{expression}"));
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexemes(
            &sources,
            &lexed,
            carrier,
            declaration_prefix.len() + comment_start,
            &context,
        );
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert!(parsed.diagnostics().is_empty(), "{context}");
        assert_add_expression(
            &sources,
            parsed.ast(),
            variable_initializer(parsed.ast(), parsed.root(), &context),
            declaration_prefix.len(),
            carrier,
            &context,
        );
        source_count += 1;

        let block_prefix = format!("{{ {FIRST_DECLARATION} ");
        let block_suffix = format!("{AFTER_DECLARATION} }}");
        let context = format!("{} block", carrier.name);
        let (sources, source_id) = add_source(format!(
            "{block_prefix}{}{}{block_suffix}",
            carrier.comment, carrier.line_break
        ));
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexemes(&sources, &lexed, carrier, block_prefix.len(), &context);
        let parsed = parse_block_twice(&sources, source_id, &context);
        assert!(parsed.diagnostics().is_empty(), "{context}");
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
                    panic!("expected local declaration for {context}")
                };
                *declaration
            })
            .collect::<Vec<_>>();
        assert_declaration(
            &sources,
            parsed.ast(),
            declarations[0],
            FIRST_DECLARATION,
            &context,
        );
        assert_declaration(
            &sources,
            parsed.ast(),
            declarations[1],
            AFTER_DECLARATION,
            &context,
        );
        source_count += 1;

        let file_prefix = format!("{FIRST_DECLARATION} ");
        let context = format!("{} file", carrier.name);
        let (sources, source_id) = add_source(format!(
            "{file_prefix}{}{}{AFTER_DECLARATION}",
            carrier.comment, carrier.line_break
        ));
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexemes(&sources, &lexed, carrier, file_prefix.len(), &context);
        let parsed = parse_file_twice(&sources, source_id, &context);
        assert!(parsed.diagnostics().is_empty(), "{context}");
        assert_eq!(parsed.roots().len(), 2, "{context}");
        assert_declaration(
            &sources,
            parsed.ast(),
            parsed.roots()[0],
            FIRST_DECLARATION,
            &context,
        );
        assert_declaration(
            &sources,
            parsed.ast(),
            parsed.roots()[1],
            AFTER_DECLARATION,
            &context,
        );
        source_count += 1;
    }

    assert_eq!(source_count, 8);
}
