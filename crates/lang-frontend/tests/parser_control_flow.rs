//! SPEC-0016 / SPEC-0123 control-flow、jump 与 super 的 Phase 1 契约测试。

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, DiagnosticDetail},
    parser::{Expression, ForBinding, ParsedBlock, ParsedExpression, Statement, WhenCondition},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{parse_block_twice, parse_expression_twice};

fn add_source(sources: &mut SourceMap, text: &str) -> SourceId {
    sources
        .add_source("control.ko", text)
        .expect("unique source")
}

fn expression(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    (sources, parsed)
}

fn block(text: &str) -> (SourceMap, ParsedBlock) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, text);
    let parsed = parse_block_twice(&sources, source_id, text);
    (sources, parsed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn expression_payload(parsed: &ParsedExpression, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("expression")
        .payload()
}

fn statement_payload(parsed: &ParsedBlock, id: StatementId) -> &Statement {
    parsed
        .ast()
        .statements()
        .get(id)
        .expect("statement")
        .payload()
}

fn root_elements(parsed: &ParsedBlock) -> &[StatementId] {
    let Statement::Block { elements } = statement_payload(parsed, parsed.root()) else {
        panic!("block root")
    };
    elements
}

#[test]
fn if_requires_else_only_in_value_context() {
    let (_, value) = expression("if (ready) 1 else 2");
    assert!(value.diagnostics().is_empty(), "{:?}", value.diagnostics());
    assert!(matches!(
        expression_payload(&value, value.root()),
        Expression::If {
            else_branch: Some(_),
            ..
        }
    ));

    let (_, missing) = expression("if (ready) 1");
    assert_eq!(codes(missing.diagnostics()), ["L0057"]);
    let span = missing.diagnostics()[0].primary_span();
    assert_eq!((span.start(), span.end()), (12, 12));

    let (_, statement) = block("{ if (ready) work() }");
    assert!(
        statement.diagnostics().is_empty(),
        "{:?}",
        statement.diagnostics()
    );
    let Statement::Expression { expression: if_id } =
        statement_payload(&statement, root_elements(&statement)[0])
    else {
        panic!("if statement")
    };
    assert!(matches!(
        statement.ast().expressions().get(*if_id).unwrap().payload(),
        Expression::If {
            else_branch: None,
            ..
        }
    ));

    let (_, initializer) = block("{ val x = if (ready) 1 }");
    assert_eq!(codes(initializer.diagnostics()), ["L0057"]);
    let (_, lambda_tail) = expression("{ if (ready) 1 }");
    assert_eq!(codes(lambda_tail.diagnostics()), ["L0057"]);
    for text in [
        "consume(if (ready) 1)",
        "result = if (ready) 1",
        "return if (ready) 1",
        "1 + if (ready) 2",
    ] {
        let (_, rejected) = expression(text);
        assert_eq!(codes(rejected.diagnostics()), ["L0057"], "{text:?}");
    }

    let (_, nested_tail) = expression("if (ready) { if (nested) 1 } else 2");
    assert_eq!(codes(nested_tail.diagnostics()), ["L0057"]);
}

#[test]
fn if_control_blocks_and_else_if_preserve_structure() {
    let (_, parsed) = expression("if (a) { val x = work(); if (x) x else x } else if (b) 2 else 3");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::If {
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = expression_payload(&parsed, parsed.root())
    else {
        panic!("outer if")
    };
    assert!(matches!(
        parsed.ast().statements().get(*then_branch).unwrap().payload(),
        Statement::ControlBody { elements } if elements.len() == 2
    ));
    let Statement::Expression { expression } = parsed
        .ast()
        .statements()
        .get(*else_branch)
        .unwrap()
        .payload()
    else {
        panic!("else if wrapper")
    };
    assert!(matches!(
        parsed
            .ast()
            .expressions()
            .get(*expression)
            .unwrap()
            .payload(),
        Expression::If { .. }
    ));
}

#[test]
fn when_forms_conditions_and_separators_are_source_ordered() {
    let text = "when (input) { is Text, !is Empty -> 1; in items -> 2\nelse -> 3 }";
    let (_, parsed) = expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::When {
        subject: Some(_),
        entries,
        ..
    } = expression_payload(&parsed, parsed.root())
    else {
        panic!("subject when")
    };
    assert_eq!(entries.len(), 3);
    assert!(matches!(
        entries[0].conditions[0],
        WhenCondition::TypeTest { negated: false, .. }
    ));
    assert!(matches!(
        entries[0].conditions[1],
        WhenCondition::TypeTest { negated: true, .. }
    ));
    assert!(matches!(
        entries[1].conditions[0],
        WhenCondition::Contains { negated: false, .. }
    ));
    assert!(entries[2].else_span.is_some());

    let (_, subjectless) = expression("when { ready -> run()\nelse -> stop() }");
    assert!(
        subjectless.diagnostics().is_empty(),
        "{:?}",
        subjectless.diagnostics()
    );
    assert!(matches!(
        expression_payload(&subjectless, subjectless.root()),
        Expression::When { subject: None, entries, .. } if entries.len() == 2
    ));

    let (_, type_test_after_binary_body) =
        expression("when (shape) { is Circle -> radius * radius\nis Point -> 0.0 }");
    assert!(
        type_test_after_binary_body.diagnostics().is_empty(),
        "{:?}",
        type_test_after_binary_body.diagnostics()
    );
    assert!(matches!(
        expression_payload(
            &type_test_after_binary_body,
            type_test_after_binary_body.root()
        ),
        Expression::When { entries, .. } if entries.len() == 2
    ));

    let (_, same_line) = expression("when { ready -> run() other -> stop() }");
    assert!(codes(same_line.diagnostics()).contains(&"L0065".to_owned()));

    let (_, nested_tokens) = expression(
        "when (input) { text -> call(\"comma, arrow ->\")\nelse -> when { ok -> 1; else -> 0 } }",
    );
    assert!(
        nested_tokens.diagnostics().is_empty(),
        "{:?}",
        nested_tokens.diagnostics()
    );
}

#[test]
fn loops_bindings_and_jumps_have_dedicated_nodes() {
    let text = "{ while (ready) { continue }; for ((key, _) in entries) { if (key) break }; loop { return } }";
    let (_, parsed) = block(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 3);
    assert!(matches!(
        statement_payload(&parsed, elements[0]),
        Statement::While { .. }
    ));
    let Statement::For { binding, .. } = statement_payload(&parsed, elements[1]) else {
        panic!("for")
    };
    assert!(matches!(
        binding,
        ForBinding::Destructuring { names, .. } if names.len() == 2
    ));
    assert!(matches!(
        statement_payload(&parsed, elements[2]),
        Statement::Loop { .. }
    ));
}

#[test]
fn return_is_local_syntax_and_newline_ends_a_bare_return() {
    let (_, parsed) = expression("{ text -> if (text) return 0\nreturn\ntext }");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::Lambda { body, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("lambda")
    };
    let Statement::LambdaBody { elements } =
        parsed.ast().statements().get(*body).unwrap().payload()
    else {
        panic!("lambda body")
    };
    assert_eq!(elements.len(), 3);
    let Statement::Expression {
        expression: return_id,
    } = parsed
        .ast()
        .statements()
        .get(elements[1])
        .unwrap()
        .payload()
    else {
        panic!("bare return")
    };
    assert!(matches!(
        parsed
            .ast()
            .expressions()
            .get(*return_id)
            .unwrap()
            .payload(),
        Expression::Return { value: None, .. }
    ));

    let (_, labeled) = expression("{ return@outer 1 }");
    assert!(!labeled.diagnostics().is_empty());
}

#[test]
fn super_member_can_continue_through_existing_postfix_parser() {
    let (_, parsed) = expression("super<Logger>.log(message).done");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        expression_payload(&parsed, parsed.root()),
        Expression::Member { .. }
    ));

    for (text, expected) in [
        ("super.log", "L0063"),
        ("super<Logger>log", "L0064"),
        ("super<Logger>.", "L0011"),
    ] {
        let (_, rejected) = expression(text);
        assert!(
            codes(rejected.diagnostics()).contains(&expected.to_owned()),
            "{text:?}: {:?}",
            rejected.diagnostics()
        );
    }
}

#[test]
fn missing_control_components_use_the_published_codes() {
    for (text, expected) in [
        ("if () 1 else 2", "L0055"),
        ("if (ready) else 2", "L0056"),
        ("when { ready run() }", "L0059"),
        ("when { -> run() }", "L0058"),
        ("{ while (ready) next }", "L0060"),
        ("{ for (in values) {} }", "L0061"),
        ("{ for (item values) {} }", "L0062"),
    ] {
        let diagnostics = if text.starts_with('{') {
            block(text).1.diagnostics().to_vec()
        } else {
            expression(text).1.diagnostics().to_vec()
        };
        assert!(
            codes(&diagnostics).contains(&expected.to_owned()),
            "{text:?}: {diagnostics:?}"
        );
    }
}

#[test]
fn missing_for_pattern_closer_leaves_in_source_body_and_following_statement() {
    let text = "{ for ((first, second in entries) { consume(first) }; sentinel() }";
    let (sources, parsed) = block(text);
    let [diagnostic] = parsed.diagnostics() else {
        panic!("one missing inner closer: {:?}", parsed.diagnostics());
    };
    assert_eq!(diagnostic.code().to_string(), "L0010");
    let offset = text.find("in entries").unwrap();
    assert_eq!(diagnostic.primary_span().start(), offset);
    assert_eq!(diagnostic.primary_span().end(), offset);
    let inner = text.find("(first").unwrap();
    assert!(
        matches!(diagnostic.details(), [DiagnosticDetail::Label(label)]
        if label.span().start() == inner && label.span().end() == inner + 1)
    );
    let [iteration, sentinel] = root_elements(&parsed) else {
        panic!("the loop and the following statement must both survive");
    };
    let Statement::For {
        binding:
            ForBinding::Destructuring {
                names,
                right_paren_span: None,
                ..
            },
        in_span,
        source,
        body,
        ..
    } = statement_payload(&parsed, *iteration)
    else {
        panic!("recovered for statement");
    };
    assert_eq!(names.len(), 2);
    assert_eq!(sources.slice(*in_span), Ok("in"));
    assert_eq!(
        sources.slice(parsed.ast().expressions().get(*source).unwrap().span()),
        Ok("entries")
    );
    assert_eq!(
        sources.slice(parsed.ast().statements().get(*body).unwrap().span()),
        Ok("{ consume(first) }")
    );
    assert_eq!(
        sources.slice(parsed.ast().statements().get(*sentinel).unwrap().span()),
        Ok("sentinel()")
    );
}

#[test]
fn unfinished_for_binding_preserves_block_and_index_closers() {
    for (text, boundary) in [
        ("{ { for ((first, /* gap */ }; sentinel() }", '}'),
        ("{ { for ((first, /* gap */ ; }; sentinel() }", ';'),
    ] {
        let (sources, parsed) = block(text);
        let [inner, sentinel] = root_elements(&parsed) else {
            panic!("the inner block must preserve the following statement");
        };
        let Statement::Block { elements } = statement_payload(&parsed, *inner) else {
            panic!("inner block");
        };
        let [iteration] = elements.as_slice() else {
            panic!("recovered for statement");
        };
        let iteration = parsed.ast().statements().get(*iteration).unwrap();
        let Statement::For { source, body, .. } = iteration.payload() else {
            panic!("for statement");
        };
        let offset = text.find(boundary).unwrap();
        assert_eq!(iteration.span().end(), text.find("first,").unwrap() + 6);
        let source = parsed.ast().expressions().get(*source).unwrap();
        let body = parsed.ast().statements().get(*body).unwrap();
        assert!(matches!(source.payload(), Expression::Error));
        assert!(matches!(body.payload(), Statement::Error));
        for span in [source.span(), body.span()] {
            assert_eq!((span.start(), span.end()), (offset, offset));
        }
        let sentinel = parsed.ast().statements().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()), Ok("sentinel()"));
    }

    let text = "owner[{ for ((first, ]";
    let (_, parsed) = expression(text);
    let node = parsed.ast().expressions().get(parsed.root()).unwrap();
    let Expression::Index { index, .. } = node.payload() else {
        panic!("outer index");
    };
    assert_eq!(node.span().end(), text.len());
    let index = parsed.ast().expressions().get(*index).unwrap();
    assert!(
        index.span().end() <= text.find(']').unwrap(),
        "{:?}",
        index.span()
    );
}

#[test]
fn grouped_when_entries_own_their_arrows_across_lines() {
    let text = "when (x) { null -> run\n(1)\n((true)) -> 2\nelse -> 3 }";
    let (sources, parsed) = expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("when expression");
    };
    let [first, grouped, otherwise] = entries.as_slice() else {
        panic!("three entries: {entries:?}");
    };
    let body = parsed.ast().statements().get(first.body).unwrap();
    assert!(
        matches!(body.payload(), Statement::Expression { expression }
        if matches!(expression_payload(&parsed, *expression), Expression::Call { .. }))
    );
    assert_eq!(sources.slice(body.span()), Ok("run\n(1)"));
    let [WhenCondition::Expression(selector)] = grouped.conditions.as_slice() else {
        panic!("one grouped selector");
    };
    let selector = parsed.ast().expressions().get(*selector).unwrap();
    assert!(matches!(selector.payload(), Expression::Group { .. }));
    assert_eq!(sources.slice(selector.span()), Ok("((true))"));
    assert_eq!(sources.slice(grouped.arrow_span), Ok("->"));
    assert_eq!(
        sources.slice(parsed.ast().statements().get(grouped.body).unwrap().span()),
        Ok("2")
    );
    assert!(otherwise.else_span.is_some());
}

#[test]
fn nested_when_selectors_keep_their_own_postfix_calls() {
    let text = "when (x) { null -> when (y) { test\n(1) -> 1; else -> 2 }; else -> 3 }";
    let (sources, parsed) = expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| matches!(node.payload(), Expression::Call { .. }).then_some(node))
        .collect::<Vec<_>>();
    let [selector] = calls.as_slice() else {
        panic!("one call");
    };
    assert_eq!(sources.slice(selector.span()), Ok("test\n(1)"));
}

#[test]
fn composite_grouped_when_selectors_keep_their_arrows() {
    for condition in [
        "((true)) && true",
        "(true), false",
        "(true) &&\n(true)",
        "((true)) && if (true) true else false",
        "((true)) && when { true -> true; else -> false }",
        "((true)) && test<Boolean>(true)",
        "((true)) && if (okay) return 1 else true",
        "((true)) && super<Interface>.check()",
        "(true)\nis Boolean",
        "(1)\nin values",
    ] {
        let text = format!("when {{ false -> produce\n(1)\n{condition} -> 2\nelse -> 3 }}");
        let (sources, parsed) = expression(&text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{condition}: {:?}",
            parsed.diagnostics()
        );
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("when expression");
        };
        let [call, composite, otherwise] = entries.as_slice() else {
            panic!("three entries: {entries:?}");
        };
        let first = parsed.ast().statements().get(call.body).unwrap();
        assert_eq!(sources.slice(first.span()), Ok("produce\n(1)"));
        assert!(
            matches!(first.payload(), Statement::Expression { expression }
            if matches!(expression_payload(&parsed, *expression), Expression::Call { .. }))
        );
        let first_condition = match composite.conditions[0] {
            WhenCondition::Expression(expression) => expression,
            ref other => panic!("expression selector: {other:?}"),
        };
        match condition {
            "(true), false" => {
                assert_eq!(composite.conditions.len(), 2);
                assert!(matches!(
                    expression_payload(&parsed, first_condition),
                    Expression::Group { .. }
                ));
            }
            "(true)\nis Boolean" => {
                assert_eq!(composite.conditions.len(), 1);
                assert!(matches!(
                    expression_payload(&parsed, first_condition),
                    Expression::TypeTest { .. }
                ));
            }
            _ => {
                assert_eq!(composite.conditions.len(), 1);
                assert!(matches!(
                    expression_payload(&parsed, first_condition),
                    Expression::Binary { .. }
                ));
            }
        }
        assert_eq!(
            sources.slice(composite.span),
            Ok(format!("{condition} -> 2").as_str())
        );
        assert_eq!(sources.slice(composite.arrow_span), Ok("->"));
        assert!(otherwise.else_span.is_some());
    }
}

#[test]
fn when_selector_probe_ignores_arrows_inside_the_previous_arm() {
    let text = "when { false -> produce\n(1) && (when { true -> true; else -> false })\n((true)) -> 2\nelse -> 3 }";
    let (sources, parsed) = expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
        panic!("when expression");
    };
    let [previous, grouped, _] = entries.as_slice() else {
        panic!("three entries: {entries:?}");
    };
    let body = parsed.ast().statements().get(previous.body).unwrap();
    assert_eq!(
        sources.slice(body.span()),
        Ok("produce\n(1) && (when { true -> true; else -> false })")
    );
    assert_eq!(sources.slice(grouped.span), Ok("((true)) -> 2"));
}

#[test]
fn when_selector_probe_preserves_other_arrow_owners() {
    for (tail, expected_body, expected_selector) in [
        ("\n{ -> true }() -> 2", "produce\n(1)", "{ -> true }()"),
        (
            " as () -> Int\ntrue -> 2",
            "produce\n(1) as () -> Int",
            "true",
        ),
    ] {
        let text = format!("when {{ false -> produce\n(1){tail}\nelse -> 3 }}");
        let (sources, parsed) = expression(&text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{tail}: {:?}",
            parsed.diagnostics()
        );
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("when expression");
        };
        let [previous, next, _] = entries.as_slice() else {
            panic!("three entries: {entries:?}");
        };
        assert_eq!(
            sources.slice(parsed.ast().statements().get(previous.body).unwrap().span()),
            Ok(expected_body)
        );
        let [WhenCondition::Expression(selector)] = next.conditions.as_slice() else {
            panic!("one selector");
        };
        assert_eq!(
            sources.slice(parsed.ast().expressions().get(*selector).unwrap().span()),
            Ok(expected_selector)
        );
        assert_eq!(sources.slice(next.arrow_span), Ok("->"));
    }
}

#[test]
fn bare_when_type_selector_keeps_its_own_entry_for_phase_two() {
    for (head, expected_body) in [
        ("when (value) { null -> produce\n(1)", "produce\n(1)"),
        ("when { false -> 1", "1"),
    ] {
        let text = format!("{head}\nis String -> 2\nelse -> 3 }}");
        let (sources, parsed) = expression(&text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let Expression::When { entries, .. } = expression_payload(&parsed, parsed.root()) else {
            panic!("when expression");
        };
        let [previous, next, _] = entries.as_slice() else {
            panic!("three entries: {entries:?}");
        };
        assert_eq!(
            sources.slice(parsed.ast().statements().get(previous.body).unwrap().span()),
            Ok(expected_body)
        );
        assert!(matches!(
            next.conditions.as_slice(),
            [WhenCondition::TypeTest { .. }]
        ));
        assert_eq!(sources.slice(next.span), Ok("is String -> 2"));
    }
}
