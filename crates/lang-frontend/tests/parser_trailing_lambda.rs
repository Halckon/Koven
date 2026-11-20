//! SPEC-0213 的同行尾 lambda 正反与 AST 规范化测试。

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{CallArgument, Expression, ParsedBlock, ParsedExpression, Statement},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{parse_block_twice, parse_expression_twice};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique test source")
}

fn parsed_expression(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source = add_source(&mut sources, "trailing-expression.ko", text);
    let parsed = parse_expression_twice(&sources, source, text);
    (sources, parsed)
}

fn parsed_block(text: &str) -> (SourceMap, ParsedBlock) {
    let mut sources = SourceMap::new();
    let source = add_source(&mut sources, "trailing-block.ko", text);
    let parsed = parse_block_twice(&sources, source, text);
    (sources, parsed)
}

fn expression(parsed: &ParsedExpression, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("expression")
        .payload()
}

fn diagnostic_fingerprints(parsed: &ParsedExpression) -> Vec<(String, usize, usize)> {
    parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            let span = diagnostic.primary_span();
            (diagnostic.code().to_string(), span.start(), span.end())
        })
        .collect()
}

fn root_call(parsed: &ParsedExpression) -> (&[lang_frontend::ast::TypeRefId], &[CallArgument]) {
    let Expression::Call {
        type_arguments,
        arguments,
        ..
    } = expression(parsed, parsed.root())
    else {
        panic!("expected call root")
    };
    (type_arguments, arguments)
}

fn block_statement(parsed: &ParsedBlock, id: StatementId) -> &Statement {
    parsed
        .ast()
        .statements()
        .get(id)
        .expect("statement")
        .payload()
}

fn assert_last_argument_is_plain_lambda(parsed: &ParsedExpression, arguments: &[CallArgument]) {
    let argument = arguments.last().expect("trailing argument");
    assert!(argument.named_prefix.is_none());
    assert!(argument.mode_marker.is_none());
    assert_eq!(
        argument.span,
        parsed
            .ast()
            .expressions()
            .get(argument.value)
            .unwrap()
            .span()
    );
    assert!(matches!(
        expression(parsed, argument.value),
        Expression::Lambda { .. }
    ));
}

#[test]
fn normalizes_plain_parenthesized_typed_member_and_chained_calls() {
    for (text, expected_arguments, expected_types, expected_calls) in [
        ("f { x -> x }", 1, 0, 1),
        ("f() { x -> x }", 1, 0, 1),
        ("f(a) { x -> x }", 2, 0, 1),
        ("f<T> { x -> x }", 1, 1, 1),
        ("f<T>() { x -> x }", 1, 1, 1),
        ("receiver.consume(1) { x -> x }", 2, 0, 1),
        ("factory().consume { x -> x }", 1, 0, 2),
        ("f { g { x -> x } }", 1, 0, 2),
        ("f { g(move { x }) }", 1, 0, 2),
    ] {
        let (_, parsed) = parsed_expression(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let (type_arguments, arguments) = root_call(&parsed);
        assert_eq!(arguments.len(), expected_arguments, "{text:?}");
        assert_eq!(type_arguments.len(), expected_types, "{text:?}");
        assert_last_argument_is_plain_lambda(&parsed, arguments);
        assert_eq!(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, node)| matches!(node.payload(), Expression::Call { .. }))
                .count(),
            expected_calls,
            "{text:?}"
        );
        assert_eq!(
            parsed
                .ast()
                .expressions()
                .get(parsed.root())
                .unwrap()
                .span()
                .end(),
            text.len(),
            "{text:?}"
        );
    }
}

#[test]
fn line_breaks_split_explicit_blocks_while_same_line_trivia_attaches() {
    for text in [
        "{ f\nblock {} }",
        "{ f\r\nblock {} }",
        "{ f // line comment\nblock {} }",
        "{ f /* line\ncomment */ block {} }",
    ] {
        let (_, parsed) = parsed_block(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let Statement::Block { elements } = block_statement(&parsed, parsed.root()) else {
            panic!("block root")
        };
        assert_eq!(elements.len(), 2, "{text:?}");
        assert!(matches!(
            block_statement(&parsed, elements[0]),
            Statement::Expression { .. }
        ));
        assert!(matches!(
            block_statement(&parsed, elements[1]),
            Statement::Block { .. }
        ));
    }

    for text in ["{ f {} }", "{ f /* same line */ {} }"] {
        let (_, parsed) = parsed_block(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let calls = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Expression::Call { .. }))
            .count();
        assert_eq!(calls, 1, "{text:?}");
    }
}

#[test]
fn rejects_second_lambda_pseudo_prefixes_and_failed_typed_trials_without_reinterpretation() {
    let (_, second) = parsed_expression("f {} {}");
    assert_eq!(
        second
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Expression::Call { .. }))
            .count(),
        1
    );
    assert_eq!(
        diagnostic_fingerprints(&second),
        [("L0013".to_owned(), 5, 6)]
    );

    for (text, expected_span) in [
        ("f name = {}", (2, 6)),
        ("f borrow {}", (2, 8)),
        ("f & {}", (2, 3)),
    ] {
        let (_, parsed) = parsed_expression(text);
        assert_eq!(
            diagnostic_fingerprints(&parsed),
            [("L0013".to_owned(), expected_span.0, expected_span.1)],
            "{text:?}"
        );
        assert!(!matches!(
            expression(&parsed, parsed.root()),
            Expression::Call { .. }
        ));
    }

    let (_, malformed) = parsed_expression("f<T,> {}");
    assert!(!matches!(
        expression(&malformed, malformed.root()),
        Expression::Call { .. }
    ));
    assert!(malformed.ast().type_refs().is_empty());
    assert_eq!(
        diagnostic_fingerprints(&malformed),
        [("L0013".to_owned(), 3, 4)]
    );
}

#[test]
fn missing_closer_utf8_and_lexical_poison_keep_existing_root_diagnostics() {
    let text = "consume { item";
    let (_, missing) = parsed_expression(text);
    assert!(matches!(
        expression(&missing, missing.root()),
        Expression::Call { .. }
    ));
    assert_eq!(
        diagnostic_fingerprints(&missing),
        [("L0010".to_owned(), text.len(), text.len())]
    );

    let utf8 = "f { \"项\"";
    let (_, missing_utf8) = parsed_expression(utf8);
    assert_eq!(
        diagnostic_fingerprints(&missing_utf8),
        [("L0010".to_owned(), utf8.len(), utf8.len())]
    );

    let (_, poison) = parsed_expression("f { \"abc");
    assert_eq!(
        diagnostic_fingerprints(&poison),
        [("L0004".to_owned(), 4, 8)]
    );
    assert!(matches!(
        expression(&poison, poison.root()),
        Expression::Call { .. }
    ));
}

#[test]
fn typed_trailing_lambda_does_not_cross_a_line_break() {
    for text in ["f<T>\n{}", "f<T>\r\n{}", "f<T> /* line\ncomment */ {}"] {
        let (_, parsed) = parsed_expression(text);
        assert!(!matches!(
            expression(&parsed, parsed.root()),
            Expression::Call { .. }
        ));
        assert!(parsed.ast().type_refs().is_empty());
    }
}

#[test]
fn explicit_block_and_parenthesized_cross_line_trailing_lambda_contract() {
    // 1. Parenthesized calls (including f<T>()) attach trailing lambda across newlines
    for (text, expected_types) in [
        ("f()\n{}", 0),
        ("f()\r\n{}", 0),
        ("f() /* line\ncomment */ {}", 0),
        ("f<T>()\n{}", 1),
        ("obj.m(1)\n{ x -> x }", 0),
    ] {
        let (_, parsed) = parsed_expression(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let (type_arguments, arguments) = root_call(&parsed);
        assert_eq!(type_arguments.len(), expected_types, "{text:?}");
        assert_last_argument_is_plain_lambda(&parsed, arguments);
    }

    // 2. Parenthesized call followed by explicit `block {}` stays two elements
    for text in [
        "{ configure()\nblock { setup() } }",
        "{ val xs = listOf(1)\nblock { use(xs) } }",
        "{ if (ok) configure()\nblock { cleanup() } }",
    ] {
        let (_, block) = parsed_block(text);
        assert!(
            block.diagnostics().is_empty(),
            "{text:?}: {:?}",
            block.diagnostics()
        );
        let Statement::Block { elements } = block_statement(&block, block.root()) else {
            panic!("block root");
        };
        assert_eq!(elements.len(), 2, "{text:?}");
        assert!(matches!(
            block_statement(&block, elements[1]),
            Statement::Block { .. }
        ));
    }

    // 3. Bare `{ ... }` statement in block/lambda is rejected with `block { ... }` guidance
    let (_, bare_block) = parsed_block("{ { setup() } }");
    assert_eq!(bare_block.diagnostics().len(), 1);
    assert_eq!(bare_block.diagnostics()[0].code().to_string(), "L0030");
    assert!(
        bare_block.diagnostics()[0]
            .message()
            .contains("block { ... }")
    );

    let (_, bare_in_lambda) = parsed_expression("{ { setup() } }");
    assert_eq!(bare_in_lambda.diagnostics().len(), 1);
    assert_eq!(bare_in_lambda.diagnostics()[0].code().to_string(), "L0032");
    assert!(
        bare_in_lambda.diagnostics()[0]
            .message()
            .contains("block { ... }")
    );

    // 4. Expression-position `block { ... }` is rejected without falling back to a call
    let (_, expr_block) = parsed_block("{ val x = block { 1 } }");
    assert!(!expr_block.diagnostics().is_empty());
    assert_eq!(expr_block.diagnostics()[0].code().to_string(), "L0009");

    // 5. `block` stays usable as ordinary identifier, call, member trailing lambda, and `block({})`
    for text in [
        "{ val block = 1\nblock(2)\nobj.block { x -> x }\nblock({ x -> x }) }",
        "{ val x = block({ 1 }) }",
    ] {
        let (_, ok) = parsed_block(text);
        assert!(
            ok.diagnostics().is_empty(),
            "{text:?}: {:?}",
            ok.diagnostics()
        );
    }

    // 6. Standalone `move { ... }` is not conflated with bare block statements
    let (_, move_stmt) = parsed_block("{ move { x } }");
    assert!(
        move_stmt.diagnostics().is_empty(),
        "{:?}",
        move_stmt.diagnostics()
    );
}
