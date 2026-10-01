//! SPEC-0234 的普通 block 换行分隔、Pratt 续行和 owner / Span 契约。

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, Item, ParsedBlock, PrefixOperator, Statement, SyntaxAst},
    source::{SourceMap, Span},
};

#[path = "support/parser_line_break_carriers.rs"]
mod parser_line_break_carriers;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_line_break_carriers::{NON_BREAK_TRIVIA, STRUCTURAL_BREAKS, validate_carrier_lexemes};
use parser_test_assertions::{
    lex_parser_source_twice, parse_block_twice, parse_declaration_twice, parse_expression_twice,
};

fn parsed(text: &str) -> (SourceMap, ParsedBlock) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("block-continuation.ko", text)
        .expect("source");
    let parsed = parse_block_twice(&sources, source_id, text);
    (sources, parsed)
}

fn parsed_ok(text: &str) -> (SourceMap, ParsedBlock) {
    let result = parsed(text);
    assert!(
        result.1.diagnostics().is_empty(),
        "{text:?}: {:?}",
        result.1.diagnostics()
    );
    result
}

fn block_elements(ast: &SyntaxAst, id: StatementId) -> &[StatementId] {
    match ast.statements().get(id).expect("block").payload() {
        Statement::Block { elements } | Statement::ControlBody { elements } => elements,
        other => panic!("expected block, got {other:?}"),
    }
}

fn expression(ast: &SyntaxAst, id: StatementId) -> ExpressionId {
    let statement = ast.statements().get(id).expect("statement");
    let Statement::Expression { expression } = statement.payload() else {
        panic!("expected expression statement: {statement:?}")
    };
    assert_eq!(
        statement.span(),
        ast.expressions()
            .get(*expression)
            .expect("expression")
            .span()
    );
    *expression
}

fn assert_span(span: Span, start: usize, text: &str) {
    assert_eq!((span.start(), span.end()), (start, start + text.len()));
}

fn assert_name(ast: &SyntaxAst, id: ExpressionId, start: usize, text: &str) {
    let node = ast.expressions().get(id).expect("name");
    assert_eq!(node.payload(), &Expression::Name);
    assert_span(node.span(), start, text);
}

fn assert_separate_tail(ast: &SyntaxAst, id: ExpressionId, start: usize, tail: &str) {
    let node = ast.expressions().get(id).expect("tail");
    assert_span(node.span(), start, tail);
    match (tail, node.payload()) {
        ("(bar)", Expression::Group { expression }) => {
            assert_name(ast, *expression, start + 1, "bar");
        }
        (
            "+b" | "-b",
            Expression::Prefix {
                operator,
                operator_span,
                operand,
            },
        ) => {
            assert_eq!(
                *operator,
                if tail == "+b" {
                    PrefixOperator::Plus
                } else {
                    PrefixOperator::Minus
                }
            );
            assert_span(*operator_span, start, &tail[..1]);
            assert_name(ast, *operand, start + 1, "b");
        }
        _ => panic!("expected independent {tail}, got {:?}", node.payload()),
    }
}

#[test]
fn structural_breaks_separate_group_and_unary_starts_with_exact_spans() {
    let mut executed = 0;
    for (head, tail) in [("foo", "(bar)"), ("a", "+b"), ("a", "-b")] {
        for carrier in STRUCTURAL_BREAKS {
            let prefix = format!("{{ {head}");
            let text = format!("{prefix}{}{tail} }}", carrier.text);
            let (sources, parsed) = parsed_ok(&text);
            let lexed = lex_parser_source_twice(&sources, parsed.source_id(), &text);
            validate_carrier_lexemes(&sources, &lexed, prefix.len(), *carrier, &text);
            let elements = block_elements(parsed.ast(), parsed.root());
            assert_eq!(elements.len(), 2, "{text:?}");
            assert_span(
                parsed
                    .ast()
                    .statements()
                    .get(parsed.root())
                    .expect("root")
                    .span(),
                0,
                &text,
            );
            assert_name(parsed.ast(), expression(parsed.ast(), elements[0]), 2, head);
            assert_separate_tail(
                parsed.ast(),
                expression(parsed.ast(), elements[1]),
                prefix.len() + carrier.text.len(),
                tail,
            );
            executed += 1;
        }
    }
    assert_eq!(executed, 18);
}

#[test]
fn whitespace_and_bare_cr_comments_keep_call_and_binary_continuations() {
    let mut executed = 0;
    for (head, tail) in [("foo", "(bar)"), ("a", "+b"), ("a", "-b")] {
        for carrier in NON_BREAK_TRIVIA {
            let prefix = format!("{{ {head}");
            let text = format!("{prefix}{}{tail} }}", carrier.text);
            let (sources, parsed) = parsed_ok(&text);
            let lexed = lex_parser_source_twice(&sources, parsed.source_id(), &text);
            validate_carrier_lexemes(&sources, &lexed, prefix.len(), *carrier, &text);
            let elements = block_elements(parsed.ast(), parsed.root());
            assert_eq!(elements.len(), 1, "{text:?}");
            let node = parsed
                .ast()
                .expressions()
                .get(expression(parsed.ast(), elements[0]))
                .expect("continued expression");
            assert_span(node.span(), 2, &text[2..text.len() - 2]);
            match node.payload() {
                Expression::Call {
                    callee, arguments, ..
                } if tail == "(bar)" => {
                    assert_name(parsed.ast(), *callee, 2, head);
                    assert_eq!(arguments.len(), 1);
                    assert_name(
                        parsed.ast(),
                        arguments[0].value,
                        prefix.len() + carrier.text.len() + 1,
                        "bar",
                    );
                }
                Expression::Binary {
                    left,
                    operator,
                    right,
                    ..
                } if tail != "(bar)" => {
                    assert_name(parsed.ast(), *left, 2, head);
                    assert_eq!(
                        *operator,
                        if tail == "+b" {
                            BinaryOperator::Add
                        } else {
                            BinaryOperator::Subtract
                        }
                    );
                    assert_name(
                        parsed.ast(),
                        *right,
                        prefix.len() + carrier.text.len() + 1,
                        "b",
                    );
                }
                other => panic!("unexpected continuation for {text:?}: {other:?}"),
            }
            executed += 1;
        }
    }
    assert_eq!(executed, 12);
}

#[test]
fn local_initializers_stop_before_the_next_group_or_unary_statement() {
    for keyword in ["val", "var"] {
        for (head, tail) in [("foo", "(bar)"), ("a", "+b"), ("a", "-b")] {
            for carrier in STRUCTURAL_BREAKS {
                let declaration = format!("{keyword} value = {head}");
                let prefix = format!("{{ {declaration}");
                let text = format!("{prefix}{}{tail}; sentinel }}", carrier.text);
                let (_, parsed) = parsed_ok(&text);
                let elements = block_elements(parsed.ast(), parsed.root());
                assert_eq!(elements.len(), 3, "{text:?}");
                let statement = parsed.ast().statements().get(elements[0]).expect("local");
                let Statement::LocalVariable { declaration: id } = statement.payload() else {
                    panic!("local variable")
                };
                let item = parsed.ast().items().get(*id).expect("variable");
                assert_eq!(statement.span(), item.span());
                assert_span(item.span(), 2, &declaration);
                let Item::Variable {
                    initializer,
                    equals_span,
                    ..
                } = item.payload()
                else {
                    panic!("variable")
                };
                assert_span(*equals_span, text.find('=').expect("equals"), "=");
                assert_name(parsed.ast(), *initializer, prefix.len() - head.len(), head);
                assert_separate_tail(
                    parsed.ast(),
                    expression(parsed.ast(), elements[1]),
                    prefix.len() + carrier.text.len(),
                    tail,
                );
                assert_name(
                    parsed.ast(),
                    expression(parsed.ast(), elements[2]),
                    text.find("sentinel").expect("sentinel"),
                    "sentinel",
                );
            }
        }
    }
}

#[test]
fn nested_and_control_blocks_apply_the_same_boundary_without_swallowing_owners() {
    for (prefix, suffix) in [
        ("{ { ", " }\nsentinel }"),
        ("{ if (ready) { ", " }\nsentinel }"),
        ("{ while (ready) { ", " }\nsentinel }"),
    ] {
        let text = format!("{prefix}foo\n(bar)\na\n+b{suffix}");
        let (_, parsed) = parsed_ok(&text);
        let root = block_elements(parsed.ast(), parsed.root());
        assert_eq!(root.len(), 2, "{text:?}");
        let inner_id = match parsed
            .ast()
            .statements()
            .get(root[0])
            .expect("owner")
            .payload()
        {
            Statement::Block { .. } => root[0],
            Statement::While { body, .. } => *body,
            Statement::Expression { expression } => match parsed
                .ast()
                .expressions()
                .get(*expression)
                .expect("if")
                .payload()
            {
                Expression::If {
                    then_branch,
                    else_branch: None,
                    ..
                } => *then_branch,
                other => panic!("if: {other:?}"),
            },
            other => panic!("owner: {other:?}"),
        };
        let inner = block_elements(parsed.ast(), inner_id);
        assert_eq!(inner.len(), 4, "{text:?}");
        let start = prefix.len();
        assert_name(
            parsed.ast(),
            expression(parsed.ast(), inner[0]),
            start,
            "foo",
        );
        assert_separate_tail(
            parsed.ast(),
            expression(parsed.ast(), inner[1]),
            start + 4,
            "(bar)",
        );
        assert_name(
            parsed.ast(),
            expression(parsed.ast(), inner[2]),
            start + 10,
            "a",
        );
        assert_separate_tail(
            parsed.ast(),
            expression(parsed.ast(), inner[3]),
            start + 12,
            "+b",
        );
        assert_name(
            parsed.ast(),
            expression(parsed.ast(), root[1]),
            text.find("sentinel").expect("sentinel"),
            "sentinel",
        );
        let inner_span = parsed
            .ast()
            .statements()
            .get(inner_id)
            .expect("inner")
            .span();
        assert_eq!(
            inner_span.end(),
            text.find("}\n").expect("inner closer") + 1
        );
    }
}

#[test]
fn unfinished_operators_and_delimiters_keep_continuing_inside_blocks() {
    for body in [
        "a +\nb",
        "a -\n(b)",
        "a +\n+b",
        "a *\nfoo\n(bar)",
        "(a\n+b)",
        "f(a\n+b)",
        "a[b\n+c]",
        "f(a,\n+b)",
        "(foo\n(bar))",
        "f(foo\n(bar))",
        "a[foo\n(bar)]",
        "a\n.member",
        "a\n?.member",
        "a\n?:b",
        "a\nand b",
        "a\n* b",
        "f<T>\n(x)",
        "f<T>\n(x)\n.member",
    ] {
        let text = format!("{{ {body} }}");
        let (_, parsed) = parsed_ok(&text);
        let elements = block_elements(parsed.ast(), parsed.root());
        let expected = if body == "a *\nfoo\n(bar)" { 2 } else { 1 };
        assert_eq!(elements.len(), expected, "{text:?}");
        let span = parsed
            .ast()
            .expressions()
            .get(expression(parsed.ast(), elements[0]))
            .expect("expression")
            .span();
        let expected_body = if expected == 2 { "a *\nfoo" } else { body };
        assert_span(span, 2, expected_body);
        if body.starts_with("f<T>") {
            let calls = parsed.ast().expressions().iter().filter(|(_, node)| matches!(node.payload(), Expression::Call { type_arguments, .. } if type_arguments.len() == 1)).count();
            assert_eq!(calls, 1, "{text:?}");
        }
    }
}

#[test]
fn standalone_expression_and_top_level_initializers_keep_their_existing_continuations() {
    for body in ["foo\n(bar)", "a\n+b", "a\n-b"] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("expression.ko", body).expect("source");
        let parsed = parse_expression_twice(&sources, id, body);
        assert!(parsed.diagnostics().is_empty(), "{body:?}");
        let node = parsed.ast().expressions().get(parsed.root()).expect("root");
        assert_span(node.span(), 0, body);
        assert!(matches!(
            node.payload(),
            Expression::Call { .. } | Expression::Binary { .. }
        ));
        let text = format!("val value = {body}");
        let id = sources.add_source("declaration.ko", &text).expect("source");
        let parsed = parse_declaration_twice(&sources, id, &text);
        assert!(parsed.diagnostics().is_empty(), "{text:?}");
        let Item::Variable { initializer, .. } = parsed
            .ast()
            .items()
            .get(parsed.root())
            .expect("variable")
            .payload()
        else {
            panic!("variable")
        };
        let node = parsed
            .ast()
            .expressions()
            .get(*initializer)
            .expect("initializer");
        assert_span(node.span(), "val value = ".len(), body);
        assert!(matches!(
            node.payload(),
            Expression::Call { .. } | Expression::Binary { .. }
        ));
    }
}

#[test]
fn semicolons_and_existing_trailing_lambda_boundaries_remain_intact() {
    for text in ["{ foo; (bar); a; +b; }", "{ foo\n(bar); a\n+b; }"] {
        let (_, parsed) = parsed_ok(text);
        assert_eq!(
            block_elements(parsed.ast(), parsed.root()).len(),
            4,
            "{text:?}"
        );
    }
    let (_, parsed) = parsed_ok("{ run { value }\n{ other } }");
    let elements = block_elements(parsed.ast(), parsed.root());
    assert_eq!(elements.len(), 2);
    assert!(matches!(
        parsed
            .ast()
            .expressions()
            .get(expression(parsed.ast(), elements[0]))
            .expect("call")
            .payload(),
        Expression::Call { .. }
    ));
    assert!(matches!(
        parsed
            .ast()
            .statements()
            .get(elements[1])
            .expect("block")
            .payload(),
        Statement::Block { .. }
    ));
}

#[test]
fn missing_group_closer_preserves_block_owner_and_exact_recovery_spans() {
    let text = "{ foo\n(bar\n+b }";
    let (_, parsed) = parsed(text);
    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    let diagnostic = &parsed.diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), "L0010");
    let closer = text.find('}').expect("closer");
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (closer, closer)
    );
    let elements = block_elements(parsed.ast(), parsed.root());
    assert_eq!(elements.len(), 2);
    assert_name(
        parsed.ast(),
        expression(parsed.ast(), elements[0]),
        2,
        "foo",
    );
    let group = parsed
        .ast()
        .expressions()
        .get(expression(parsed.ast(), elements[1]))
        .expect("group");
    assert_span(group.span(), 6, "(bar\n+b");
    let Expression::Group { expression: inner } = group.payload() else {
        panic!("group")
    };
    assert!(matches!(
        parsed
            .ast()
            .expressions()
            .get(*inner)
            .expect("binary")
            .payload(),
        Expression::Binary {
            operator: BinaryOperator::Add,
            ..
        }
    ));
    assert_span(
        parsed
            .ast()
            .statements()
            .get(parsed.root())
            .expect("root")
            .span(),
        0,
        text,
    );
}

#[test]
fn repeated_boundaries_are_flat_and_nested_delimiters_do_not_leak_block_stops() {
    let count = 256;
    let text = format!("{{\n{} }}", "foo\n(bar)\na\n+b\n".repeat(count));
    let (_, parsed) = parsed_ok(&text);
    assert_eq!(block_elements(parsed.ast(), parsed.root()).len(), count * 4);
    let depth = 128;
    let body = format!("{}a\n+b{}", "(".repeat(depth), ")".repeat(depth));
    let text = format!("{{ {body}\n(c) }}");
    let (_, parsed) = parsed_ok(&text);
    let elements = block_elements(parsed.ast(), parsed.root());
    assert_eq!(elements.len(), 2);
    assert_span(
        parsed
            .ast()
            .expressions()
            .get(expression(parsed.ast(), elements[0]))
            .expect("deep group")
            .span(),
        2,
        &body,
    );
    assert_eq!(
        parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| matches!(
                node.payload(),
                Expression::Binary {
                    operator: BinaryOperator::Add,
                    ..
                }
            ))
            .count(),
        1
    );
}

#[test]
fn lambda_body_scope_is_unchanged_but_its_nested_block_has_its_own_boundary() {
    // SPEC-0234 只实施普通 block；lambda 顶层 body 的换行语法留给后续切片。
    for body in ["foo\n(bar)", "a\n+b", "a\n-b", "val value = foo\n(bar)"] {
        let text = format!("{{ -> {body} }}");
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("lambda-scope.ko", &text)
            .expect("source");
        let parsed = parse_expression_twice(&sources, id, &text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let Expression::Lambda { body: id, .. } = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("lambda")
            .payload()
        else {
            panic!("lambda")
        };
        let Statement::LambdaBody { elements } =
            parsed.ast().statements().get(*id).expect("body").payload()
        else {
            panic!("lambda body")
        };
        assert_eq!(elements.len(), 1, "{text:?}");
        assert_span(
            parsed
                .ast()
                .statements()
                .get(elements[0])
                .expect("element")
                .span(),
            "{ -> ".len(),
            body,
        );
    }
    let text = "{ -> { foo\n(bar) } }";
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("lambda-nested-block.ko", text)
        .expect("source");
    let parsed = parse_expression_twice(&sources, id, text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let blocks = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Statement::Block { elements } => Some(elements),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].len(), 2);
    assert_name(
        parsed.ast(),
        expression(parsed.ast(), blocks[0][0]),
        7,
        "foo",
    );
    assert_separate_tail(
        parsed.ast(),
        expression(parsed.ast(), blocks[0][1]),
        11,
        "(bar)",
    );
}

#[test]
fn for_header_delimiters_suspend_inherited_block_line_breaks() {
    for (prefix, suffix) in [
        ("{ for (x in ", ") {} }"),
        ("{ if (ready) { for (x in ", ") {} } }"),
        ("{ when { else -> { for (x in ", ") {} } } }"),
    ] {
        for newline in ["\n", "\r\n"] {
            for (head, tail) in [("foo", "(bar)"), ("a", "+b"), ("a", "-b")] {
                let source_text = format!("{head}{newline}{tail}");
                let text = format!("{prefix}{source_text}{suffix}");
                let (_, parsed) = parsed_ok(&text);
                let loops = parsed
                    .ast()
                    .statements()
                    .iter()
                    .filter_map(|(_, node)| match node.payload() {
                        Statement::For { source, body, .. } => Some((*source, *body)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(loops.len(), 1, "{text:?}");
                let (source, body) = loops[0];
                let node = parsed.ast().expressions().get(source).expect("for source");
                assert_span(node.span(), prefix.len(), &source_text);
                match node.payload() {
                    Expression::Call {
                        callee, arguments, ..
                    } if tail == "(bar)" => {
                        assert_name(parsed.ast(), *callee, prefix.len(), head);
                        assert_eq!(arguments.len(), 1);
                        assert_name(
                            parsed.ast(),
                            arguments[0].value,
                            prefix.len() + head.len() + newline.len() + 1,
                            "bar",
                        );
                    }
                    Expression::Binary {
                        left,
                        operator,
                        operator_span,
                        right,
                    } if tail != "(bar)" => {
                        assert_name(parsed.ast(), *left, prefix.len(), head);
                        assert_eq!(
                            *operator,
                            if tail == "+b" {
                                BinaryOperator::Add
                            } else {
                                BinaryOperator::Subtract
                            }
                        );
                        let operator_start = prefix.len() + head.len() + newline.len();
                        assert_span(*operator_span, operator_start, &tail[..1]);
                        assert_name(parsed.ast(), *right, operator_start + 1, "b");
                    }
                    other => panic!("for source continuation for {text:?}: {other:?}"),
                }
                assert!(block_elements(parsed.ast(), body).is_empty());
                assert_span(
                    parsed
                        .ast()
                        .statements()
                        .get(body)
                        .expect("loop body")
                        .span(),
                    prefix.len() + source_text.len() + 2,
                    "{}",
                );
                assert_span(
                    parsed
                        .ast()
                        .statements()
                        .get(parsed.root())
                        .expect("root")
                        .span(),
                    0,
                    &text,
                );
            }
        }
    }
}
