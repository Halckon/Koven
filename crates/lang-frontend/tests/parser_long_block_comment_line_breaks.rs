//! SPEC-0168 的超长 UTF-8 block comment 非嵌套与逻辑换行矩阵。

use lang_frontend::{
    ast::{ExpressionId, ItemId},
    lexer::{LexedFile, LexemeKind, TriviaKind},
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

struct CommentCase {
    name: &'static str,
    comment: String,
    payload_start: usize,
    payload_end: usize,
    line_break_start: usize,
    line_break_end: usize,
    line_break: &'static str,
}

fn comment_case(name: &'static str, line_break: &'static str) -> CommentCase {
    let payload = "界".repeat(UTF8_SCALARS);
    assert_eq!(payload.len(), UTF8_BYTES);

    let mut comment = "/* nested /* \"${ignored}\" // marker ".to_owned();
    let payload_start = comment.len();
    comment.push_str(&payload);
    let payload_end = comment.len();
    let line_break_start = comment.len();
    comment.push_str(line_break);
    let line_break_end = comment.len();
    comment.push_str("tail */");

    assert_eq!(comment.matches("/*").count(), 2);
    assert_eq!(comment.matches("*/").count(), 1);
    CommentCase {
        name,
        comment,
        payload_start,
        payload_end,
        line_break_start,
        line_break_end,
        line_break,
    }
}

fn comment_cases() -> Vec<CommentCase> {
    vec![
        comment_case("deep LF", "\n"),
        comment_case("deep CRLF", "\r\n"),
    ]
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-long-block-comment-line-breaks.ko", source)
        .expect("long block comment source name must be unique");
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

fn assert_comment_lexeme(
    sources: &SourceMap,
    lexed: &LexedFile,
    case: &CommentCase,
    comment_start: usize,
    context: &str,
) {
    assert!(lexed.diagnostics().is_empty(), "{context}");
    let comments = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Trivia(TriviaKind::BlockComment)))
        .collect::<Vec<_>>();
    assert_eq!(comments.len(), 1, "{context}");
    let span = comments[0].span();
    assert_span(
        span,
        comment_start,
        comment_start + case.comment.len(),
        context,
    );
    assert_slice(sources, span, &case.comment, context);

    let payload_span = sources
        .span(
            span.source_id(),
            comment_start + case.payload_start,
            comment_start + case.payload_end,
        )
        .expect("payload Span must be source-valid");
    let payload = sources
        .slice(payload_span)
        .unwrap_or_else(|error| panic!("payload slice failed for {context}: {error}"));
    assert_eq!(payload.len(), UTF8_BYTES, "{context}");
    assert_eq!(payload.chars().count(), UTF8_SCALARS, "{context}");
    assert!(payload.chars().all(|scalar| scalar == '界'), "{context}");

    let line_break_span = sources
        .span(
            span.source_id(),
            comment_start + case.line_break_start,
            comment_start + case.line_break_end,
        )
        .expect("line-break Span must be source-valid");
    assert_slice(sources, line_break_span, case.line_break, context);
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
    comment_len: usize,
    context: &str,
) {
    let comment_start = expression_start + "left ".len();
    let plus_start = comment_start + comment_len + 1;
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
fn every_public_entry_preserves_long_non_nested_comments_and_their_deep_line_breaks() {
    let cases = comment_cases();
    assert_eq!(cases.len(), 2);
    let mut source_count = 0;

    for case in &cases {
        let expression = format!("left {} + right", case.comment);
        let comment_start = "left ".len();
        let context = format!("{} expression", case.name);
        let (sources, source_id) = add_source(expression);
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexeme(&sources, &lexed, case, comment_start, &context);
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert!(parsed.diagnostics().is_empty(), "{context}");
        assert_add_expression(
            &sources,
            parsed.ast(),
            parsed.root(),
            0,
            case.comment.len(),
            &context,
        );
        source_count += 1;

        let declaration_prefix = "val result = ";
        let expression = format!("left {} + right", case.comment);
        let context = format!("{} declaration", case.name);
        let (sources, source_id) = add_source(format!("{declaration_prefix}{expression}"));
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexeme(
            &sources,
            &lexed,
            case,
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
            case.comment.len(),
            &context,
        );
        source_count += 1;

        let block_prefix = format!("{{ {FIRST_DECLARATION} ");
        let block_suffix = format!(" {AFTER_DECLARATION} }}");
        let context = format!("{} block", case.name);
        let (sources, source_id) =
            add_source(format!("{block_prefix}{}{block_suffix}", case.comment));
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexeme(&sources, &lexed, case, block_prefix.len(), &context);
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
        let file_suffix = format!(" {AFTER_DECLARATION}");
        let context = format!("{} file", case.name);
        let (sources, source_id) =
            add_source(format!("{file_prefix}{}{file_suffix}", case.comment));
        let lexed = lex_parser_source_twice(&sources, source_id, &context);
        assert_comment_lexeme(&sources, &lexed, case, file_prefix.len(), &context);
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
