//! SPEC-0016 / SPEC-0123 control-flow、jump 与 super 的 Phase 1 契约测试。

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
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
    let (_, parsed) = expression("if (a) { val x = work() if (x) x else x } else if (b) 2 else 3");
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
    let text = "{ while (ready) { continue } for ((key, _) in entries) { if (key) break } loop { return } }";
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
