//! SPEC-0010 / SPEC-0120 的 lambda literal、上下文判定与恢复契约测试。

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    parser::{
        BinaryOperator, Expression, Item, ParsedBlock, ParsedDeclaration, ParsedExpression,
        ParserInternalError, Statement, StringPart, parse_expression,
    },
    source::{SourceError, SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    assert_parser_error_twice, lex_parser_source_twice, parse_block_twice, parse_declaration_twice,
    parse_expression_twice,
};

fn source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique test source")
}

fn parsed_expression(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let id = source(&mut sources, "lambda.ko", text);
    let parsed = parse_expression_twice(&sources, id, text);
    (sources, parsed)
}

fn parsed_block(text: &str) -> (SourceMap, ParsedBlock) {
    let mut sources = SourceMap::new();
    let id = source(&mut sources, "block.ko", text);
    let parsed = parse_block_twice(&sources, id, text);
    (sources, parsed)
}

fn parsed_declaration(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let id = source(&mut sources, "declaration.ko", text);
    let parsed = parse_declaration_twice(&sources, id, text);
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

fn statement(parsed: &ParsedExpression, id: StatementId) -> &Statement {
    parsed
        .ast()
        .statements()
        .get(id)
        .expect("statement")
        .payload()
}

fn lambda(
    parsed: &ParsedExpression,
    id: ExpressionId,
) -> (
    Option<lang_frontend::source::Span>,
    &[lang_frontend::source::Span],
    Option<lang_frontend::source::Span>,
    StatementId,
) {
    let Expression::Lambda {
        move_span,
        parameters,
        arrow_span,
        body,
    } = expression(parsed, id)
    else {
        panic!("expected lambda, got {:?}", expression(parsed, id));
    };
    (*move_span, parameters, *arrow_span, *body)
}

fn fingerprints(diagnostics: &[Diagnostic]) -> Vec<(String, Severity, String, usize, usize)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let span = diagnostic.primary_span();
            (
                diagnostic.code().to_string(),
                diagnostic.severity(),
                diagnostic.message().to_owned(),
                span.start(),
                span.end(),
            )
        })
        .collect()
}

#[test]
fn header_three_states_and_move_preserve_only_real_spans() {
    for (text, expected_parameters, expected_arrow, expected_move) in [
        ("{}", Vec::<&str>::new(), None, None),
        ("{ -> }", vec![], Some("->"), None),
        ("{ x, y -> x + y }", vec!["x", "y"], Some("->"), None),
        ("move { x }", vec![], None, Some("move")),
    ] {
        let (sources, parsed) = parsed_expression(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let root = parsed.ast().expressions().get(parsed.root()).expect("root");
        assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
        let (move_span, parameters, arrow_span, body) = lambda(&parsed, parsed.root());
        assert_eq!(
            move_span.map(|span| sources.slice(span).unwrap()),
            expected_move
        );
        assert_eq!(
            arrow_span.map(|span| sources.slice(span).unwrap()),
            expected_arrow
        );
        assert_eq!(
            parameters
                .iter()
                .map(|span| sources.slice(*span).unwrap())
                .collect::<Vec<_>>(),
            expected_parameters
        );
        let body_node = parsed
            .ast()
            .statements()
            .get(body)
            .expect("typed lambda body");
        assert_eq!(body_node.span().source_id(), parsed.source_id());
        assert!(matches!(body_node.payload(), Statement::LambdaBody { .. }));
    }
}

#[test]
fn lambda_body_has_ordered_elements_without_duplicate_tail_or_block_mode() {
    let (_, parsed) = parsed_expression("{ {} x }");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (_, _, _, body) = lambda(&parsed, parsed.root());
    let Statement::LambdaBody { elements } = statement(&parsed, body) else {
        panic!("lambda body")
    };
    assert_eq!(elements.len(), 2);
    assert!(matches!(
        statement(&parsed, elements[0]),
        Statement::Block { .. }
    ));
    assert!(matches!(
        statement(&parsed, elements[1]),
        Statement::Expression { .. }
    ));
    assert_eq!(
        parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Statement::LambdaBody { .. }))
            .count(),
        1
    );

    let (_, nested) = parsed_expression("{ ({ -> x }) }");
    assert!(
        nested.diagnostics().is_empty(),
        "{:?}",
        nested.diagnostics()
    );
    assert_eq!(
        nested
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Statement::LambdaBody { .. }))
            .count(),
        2
    );

    for (body_id, _) in nested
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Statement::LambdaBody { .. }))
    {
        let references = nested
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| {
                matches!(node.payload(), Expression::Lambda { body, .. } if *body == body_id)
            })
            .count();
        assert_eq!(references, 1, "each LambdaBody must have one Lambda owner");
    }
    for (_, node) in nested.ast().statements().iter() {
        let elements = match node.payload() {
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => elements,
            Statement::Error
            | Statement::LocalVariable { .. }
            | Statement::LocalDestructuring { .. }
            | Statement::While { .. }
            | Statement::For { .. }
            | Statement::Loop { .. }
            | Statement::Expression { .. } => {
                continue;
            }
        };
        assert!(elements.iter().all(|id| !matches!(
            nested.ast().statements().get(*id).unwrap().payload(),
            Statement::LambdaBody { .. }
        )));
    }
}

#[test]
fn expression_contexts_commit_lambda_while_block_dispatch_keeps_nested_block() {
    let (_, call) = parsed_expression("f({})");
    assert!(call.diagnostics().is_empty(), "{:?}", call.diagnostics());
    let Expression::Call { arguments, .. } = expression(&call, call.root()) else {
        panic!("call")
    };
    assert!(matches!(
        expression(&call, arguments[0].value),
        Expression::Lambda { .. }
    ));

    let (_, declaration) = parsed_declaration("val f = {}");
    assert!(
        declaration.diagnostics().is_empty(),
        "{:?}",
        declaration.diagnostics()
    );
    let Item::Variable { initializer, .. } = declaration
        .ast()
        .items()
        .get(declaration.root())
        .unwrap()
        .payload()
    else {
        panic!("variable")
    };
    assert!(matches!(
        declaration
            .ast()
            .expressions()
            .get(*initializer)
            .unwrap()
            .payload(),
        Expression::Lambda { .. }
    ));

    let (_, nested_block) = parsed_block("{{ x }}");
    assert!(
        nested_block.diagnostics().is_empty(),
        "{:?}",
        nested_block.diagnostics()
    );
    let Statement::Block { elements } = nested_block
        .ast()
        .statements()
        .get(nested_block.root())
        .unwrap()
        .payload()
    else {
        panic!("block")
    };
    assert!(matches!(
        nested_block
            .ast()
            .statements()
            .get(elements[0])
            .unwrap()
            .payload(),
        Statement::Block { .. }
    ));

    let (_, grouped) = parsed_block("{ ({ x }) }");
    assert!(
        grouped.diagnostics().is_empty(),
        "{:?}",
        grouped.diagnostics()
    );
    let Statement::Block { elements } = grouped
        .ast()
        .statements()
        .get(grouped.root())
        .unwrap()
        .payload()
    else {
        panic!("block")
    };
    let Statement::Expression {
        expression: grouped_id,
    } = grouped
        .ast()
        .statements()
        .get(elements[0])
        .unwrap()
        .payload()
    else {
        panic!("statement")
    };
    assert!(matches!(
        grouped
            .ast()
            .expressions()
            .get(*grouped_id)
            .unwrap()
            .payload(),
        Expression::Group { .. }
    ));

    let (_, split) = parsed_block("{ x { y } }");
    assert!(split.diagnostics().is_empty(), "{:?}", split.diagnostics());
    let Statement::Block { elements } = split
        .ast()
        .statements()
        .get(split.root())
        .unwrap()
        .payload()
    else {
        panic!("block")
    };
    assert_eq!(elements.len(), 2);
    assert!(matches!(
        split.ast().statements().get(elements[1]).unwrap().payload(),
        Statement::Block { .. }
    ));

    let (_, operand) = parsed_block("{ x + { y } }");
    assert!(
        operand.diagnostics().is_empty(),
        "{:?}",
        operand.diagnostics()
    );
    let Statement::Block { elements } = operand
        .ast()
        .statements()
        .get(operand.root())
        .unwrap()
        .payload()
    else {
        panic!("block")
    };
    let Statement::Expression { expression: binary } = operand
        .ast()
        .statements()
        .get(elements[0])
        .unwrap()
        .payload()
    else {
        panic!("expression")
    };
    assert!(matches!(
        operand.ast().expressions().get(*binary).unwrap().payload(),
        Expression::Binary {
            operator: BinaryOperator::Add,
            ..
        }
    ));
}

#[test]
fn lambda_accepts_existing_postfix_chain_but_not_trailing_lambda_call_sugar() {
    let (_, parsed) = parsed_expression("{}(x).member[0]!!::ref");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        expression(&parsed, parsed.root()),
        Expression::CallableReference { .. }
    ));

    let (_, trailing) = parsed_expression("f {}");
    assert!(
        trailing
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0013")
    );
    assert!(!matches!(
        expression(&trailing, trailing.root()),
        Expression::Call { .. }
    ));
}

#[test]
fn move_requires_a_brace_but_trivia_before_a_real_lambda_is_irrelevant() {
    let (sources, parsed) = parsed_expression("move /* comment */ {}");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (move_span, _, _, _) = lambda(&parsed, parsed.root());
    assert_eq!(
        sources.slice(move_span.expect("move span")).unwrap(),
        "move"
    );

    let (_, rejected) = parsed_expression("move x");
    assert!(!rejected.diagnostics().is_empty());
    assert!(!matches!(
        expression(&rejected, rejected.root()),
        Expression::Lambda { .. }
    ));
}

#[test]
fn strict_header_lookalikes_permanently_fail_to_zero_state() {
    for text in [
        "{ x: T -> x }",
        "{ x = y -> z }",
        "{ val x -> x }",
        "{ var x -> x }",
        "{ (x) -> z }",
        "{ ,x -> z }",
        "{ x,,y -> z }",
        "{ x, -> z }",
        "{ x y -> z }",
        "{ [x] -> z }",
        "{ \"x -> y\" -> z }",
        r#"{ "${x -> y}" -> z }"#,
    ] {
        let (_, parsed) = parsed_expression(text);
        let (_, parameters, arrow_span, _) = lambda(&parsed, parsed.root());
        assert!(parameters.is_empty() && arrow_span.is_none(), "{text:?}");
    }
}

#[test]
fn function_type_arrow_and_nested_commas_belong_to_their_subgrammar() {
    for text in [
        "{ source as () -> Int }",
        "{ f(x, y) }",
        "{ (f(x, y)) }",
        "{ values[index(x, y)] }",
    ] {
        let (_, parsed) = parsed_expression(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        assert!(
            !parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0032"),
            "{text:?}"
        );
    }
}

#[test]
fn string_interpolation_preserves_grouped_lambda_call_as_typed_structure() {
    let text = r#""${({ x -> x })(input)}""#;
    let (sources, parsed) = parsed_expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );

    let Expression::String { parts } = expression(&parsed, parsed.root()) else {
        panic!("string root")
    };
    let [
        StringPart::Interpolation {
            span,
            expression: interpolation,
        },
    ] = parts.as_slice()
    else {
        panic!("single interpolation part: {parts:?}")
    };
    assert_eq!(
        sources.slice(*span).expect("interpolation span"),
        "${({ x -> x })(input)}"
    );

    let Expression::Call {
        callee, arguments, ..
    } = expression(&parsed, *interpolation)
    else {
        panic!("interpolation call")
    };
    assert_eq!(arguments.len(), 1);
    assert!(matches!(
        expression(&parsed, arguments[0].value),
        Expression::Name
    ));
    let Expression::Group {
        expression: grouped,
    } = expression(&parsed, *callee)
    else {
        panic!("grouped lambda callee")
    };
    let (_, parameters, arrow_span, body) = lambda(&parsed, *grouped);
    assert_eq!(
        parameters
            .iter()
            .map(|span| sources.slice(*span).expect("parameter span"))
            .collect::<Vec<_>>(),
        ["x"]
    );
    assert_eq!(
        sources.slice(arrow_span.expect("header arrow")).unwrap(),
        "->"
    );
    let Statement::LambdaBody { elements } = statement(&parsed, body) else {
        panic!("lambda body")
    };
    assert_eq!(elements.len(), 1);
    let Statement::Expression { expression: body } = statement(&parsed, elements[0]) else {
        panic!("lambda body expression")
    };
    assert!(matches!(expression(&parsed, *body), Expression::Name));
}

#[test]
fn failed_header_with_missing_separator_reports_body_errors_in_order() {
    let (_, parsed) = parsed_expression("{ x y -> z }");
    assert_eq!(
        fingerprints(parsed.diagnostics())
            .iter()
            .map(|entry| (entry.0.as_str(), entry.3, entry.4))
            .collect::<Vec<_>>(),
        [("L0013", 4, 5), ("L0032", 6, 8)]
    );
    let (_, parameters, arrow, _) = lambda(&parsed, parsed.root());
    assert!(parameters.is_empty() && arrow.is_none());
}

#[test]
fn lambda_body_diagnostics_have_fixed_codes_messages_spans_and_error_nodes() {
    for (text, code, message, start, end) in [
        ("{ : }", "L0031", "expected lambda body element", 2, 3),
        ("{ , }", "L0032", "unsupported lambda body form", 2, 3),
        ("{ -> -> }", "L0032", "unsupported lambda body form", 5, 7),
        ("{ x -> , }", "L0032", "unsupported lambda body form", 7, 8),
    ] {
        let (_, parsed) = parsed_expression(text);
        assert!(
            fingerprints(parsed.diagnostics()).contains(&(
                code.to_owned(),
                Severity::Error,
                message.to_owned(),
                start,
                end
            )),
            "{text:?}: {:?}",
            fingerprints(parsed.diagnostics())
        );
        let (_, _, _, body) = lambda(&parsed, parsed.root());
        let Statement::LambdaBody { elements } = statement(&parsed, body) else {
            panic!("body")
        };
        assert!(
            elements
                .iter()
                .any(|id| matches!(statement(&parsed, *id), Statement::Error)),
            "{text:?}"
        );
    }
}

#[test]
fn unsupported_body_introducers_use_lambda_specific_diagnostic() {
    for text in ["{ const val x = 1 }", "{ fun f(): Unit }", "{ class X }"] {
        let (_, parsed) = parsed_expression(text);
        assert!(
            parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0032"
                    && diagnostic.message() == "unsupported lambda body form"),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        assert!(
            !parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0030"),
            "{text:?}"
        );
    }
}

#[test]
fn initializer_tail_is_not_reinterpreted_as_a_second_lambda_element() {
    for text in [
        "{ val x = 1 x }",
        "{ p -> val x = p x }",
        "{ val f = { x } x }",
    ] {
        let (_, parsed) = parsed_expression(text);
        assert!(
            parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0013"),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn terminal_lexer_owner_errors_do_not_gain_lambda_or_closer_cascades() {
    for (text, expected_code) in [
        ("{ \"abc", "L0004"),
        (r#"{ "${answer"#, "L0005"),
        ("{ \"abc\\", "L0006"),
    ] {
        let (_, parsed) = parsed_expression(text);
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [expected_code],
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        assert!(
            !codes
                .iter()
                .any(|code| { matches!(code.as_str(), "L0010" | "L0031" | "L0032") })
        );
    }
}

#[test]
fn owner_recovery_keeps_lambda_commas_and_closers_at_the_correct_level() {
    let (_, parsed) = parsed_expression("f({ x, y }, z)");
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0032")
            .count(),
        1
    );
    let Expression::Call { arguments, .. } = expression(&parsed, parsed.root()) else {
        panic!("call")
    };
    assert_eq!(arguments.len(), 2);
    assert!(matches!(
        expression(&parsed, arguments[0].value),
        Expression::Lambda { .. }
    ));
    assert!(matches!(
        expression(&parsed, arguments[1].value),
        Expression::Name
    ));

    let (_, shared) = parsed_block("{{ x }");
    assert!(
        shared
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0010")
    );

    let (_, missing) = parsed_expression("{ x");
    assert!(
        missing
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0010")
    );

    for (text, expected_closers) in [("f({ a[x )", 2), ("f({ { x )", 2), ("f({ val x = 1 )", 1)] {
        let (_, inherited) = parsed_expression(text);
        let closer_spans = inherited
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0010")
            .map(|diagnostic| {
                (
                    diagnostic.primary_span().start(),
                    diagnostic.primary_span().end(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            closer_spans,
            vec![(text.len() - 1, text.len() - 1); expected_closers],
            "{text:?}: {:?}",
            inherited.diagnostics()
        );
        assert!(
            !inherited.diagnostics().iter().any(|diagnostic| {
                matches!(diagnostic.code().to_string().as_str(), "L0013" | "L0029")
            }),
            "{text:?}: {:?}",
            inherited.diagnostics()
        );
        assert!(matches!(
            expression(&inherited, inherited.root()),
            Expression::Call { .. }
        ));
    }
}

#[test]
fn lambda_is_deterministic_across_source_order_and_preserves_source_identity() {
    fn shape(noise_first: bool) -> (usize, usize, Vec<(String, usize, usize)>) {
        let text = "move { x, y -> ({ -> x })(y) }";
        let mut sources = SourceMap::new();
        if noise_first {
            source(&mut sources, "noise.ko", "noise");
        }
        let id = source(&mut sources, "case.ko", text);
        let parsed = parse_expression_twice(&sources, id, text);
        assert_eq!(parsed.source_id(), id);
        (
            parsed.ast().expressions().len(),
            parsed.ast().statements().len(),
            parsed
                .diagnostics()
                .iter()
                .map(|d| {
                    (
                        d.code().to_string(),
                        d.primary_span().start(),
                        d.primary_span().end(),
                    )
                })
                .collect(),
        )
    }
    assert_eq!(shape(false), shape(true));

    let mut owner = SourceMap::new();
    let id = source(&mut owner, "owner.ko", "{}");
    let lexed = lex_parser_source_twice(&owner, id, "foreign lambda source");
    let mut foreign = SourceMap::new();
    source(&mut foreign, "foreign.ko", "{}");
    assert_parser_error_twice(
        &foreign,
        &lexed,
        ParserInternalError::Source(SourceError::InvalidSourceId { source_id: id }),
        "foreign lambda source",
        parse_expression,
    );
}

#[test]
fn lambda_and_nested_blocks_use_the_shared_recursion_budget_without_panicking() {
    std::thread::Builder::new()
        .name("lambda-small-caller".to_owned())
        .stack_size(64 * 1024)
        .spawn(|| {
            let (_, parsed) = parsed_expression("{ ({ x -> x })(input) }");
            assert!(
                parsed.diagnostics().is_empty(),
                "{:?}",
                parsed.diagnostics()
            );
        })
        .expect("small caller thread must start")
        .join()
        .expect("parser must isolate recursive work from the caller stack");

    let text = format!("{}x{}", "{ ".repeat(1_100), " }".repeat(1_100));
    let mut sources = SourceMap::new();
    let id = source(&mut sources, "deep.ko", &text);
    let lexed = lex_parser_source_twice(&sources, id, "lambda nesting budget");
    assert_parser_error_twice(
        &sources,
        &lexed,
        ParserInternalError::NestingLimitExceeded { limit: 1024 },
        "lambda nesting budget",
        parse_expression,
    );
}
