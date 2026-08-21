//! SPEC-0155 的四公开 Parser 入口大规模 lexical-owner 压力矩阵。

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    parser::{Expression, ExpressionAst, Item, Statement, StringPart},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    parse_block_twice, parse_declaration_twice, parse_expression_twice, parse_file_twice,
};

const OWNER_COUNT: usize = 4_096;
const VALID_OWNER: &str = "\"${x}\"";
const RECOVERED_OWNER: &str = "\"${}\"";

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-owner-stress.ko", source)
        .expect("owner stress source name must be unique");
    (sources, source_id)
}

fn repeated(separator: &str, owner: &str) -> String {
    (0..OWNER_COUNT)
        .map(|_| owner)
        .collect::<Vec<_>>()
        .join(separator)
}

fn local_variables(owner: &str) -> String {
    (0..OWNER_COUNT)
        .map(|index| format!("val x{index} = {owner}\n"))
        .collect()
}

fn assert_interpolation_nodes(ast: &ExpressionAst, inner_is_error: bool) {
    let mut strings = 0;
    for (_, node) in ast.expressions().iter() {
        let Expression::String { parts } = node.payload() else {
            continue;
        };
        strings += 1;
        assert_eq!(parts.len(), 1);
        let StringPart::Interpolation { expression, .. } = &parts[0] else {
            panic!("owner-rich string must retain its interpolation")
        };
        let inner = ast
            .expressions()
            .get(*expression)
            .expect("interpolation expression");
        assert_eq!(matches!(inner.payload(), Expression::Error), inner_is_error);
        if !inner_is_error {
            assert!(matches!(inner.payload(), Expression::Name));
        }
    }
    assert_eq!(strings, OWNER_COUNT);
}

fn assert_call_arguments(ast: &ExpressionAst, expression: ExpressionId) {
    let Expression::Call { arguments, .. } = ast
        .expressions()
        .get(expression)
        .expect("owner-rich call expression")
        .payload()
    else {
        panic!("owner-rich expression must remain a call")
    };
    assert_eq!(arguments.len(), OWNER_COUNT);
}

fn assert_diagnostic_series(diagnostics: &[Diagnostic]) {
    assert_eq!(
        diagnostics.len(),
        OWNER_COUNT,
        "unexpected owner recovery diagnostics: {:?}",
        diagnostics
            .iter()
            .take(4)
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ))
            .collect::<Vec<_>>()
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code().to_string() == "L0009")
    );
    assert!(
        diagnostics
            .windows(2)
            .all(|window| window[0].primary_span().start() < window[1].primary_span().start())
    );
}

#[test]
fn every_public_entry_preserves_each_large_legal_interpolation_owner() {
    let arguments = repeated(",", VALID_OWNER);
    let (sources, source_id) = add_source(format!("call({arguments})"));
    let parsed = parse_expression_twice(&sources, source_id, "owner-rich expression");
    assert!(parsed.diagnostics().is_empty());
    assert_call_arguments(parsed.ast(), parsed.root());
    assert_interpolation_nodes(parsed.ast(), false);

    let (sources, source_id) = add_source(format!("val result = call({arguments})"));
    let parsed = parse_declaration_twice(&sources, source_id, "owner-rich declaration");
    assert!(parsed.diagnostics().is_empty());
    let Item::Variable { initializer, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("owner-rich declaration root")
        .payload()
    else {
        panic!("owner-rich declaration must remain a variable")
    };
    assert_call_arguments(parsed.ast(), *initializer);
    assert_interpolation_nodes(parsed.ast(), false);

    let body = local_variables(VALID_OWNER);
    let (sources, source_id) = add_source(format!("{{\n{body}}}"));
    let parsed = parse_block_twice(&sources, source_id, "owner-rich block");
    assert!(parsed.diagnostics().is_empty());
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(parsed.root())
        .expect("owner-rich block root")
        .payload()
    else {
        panic!("owner-rich block must retain its root")
    };
    assert_eq!(elements.len(), OWNER_COUNT);
    assert!(elements.iter().all(|element| matches!(
        parsed.ast().statements().get(*element),
        Ok(node) if matches!(node.payload(), Statement::LocalVariable { .. })
    )));
    assert_interpolation_nodes(parsed.ast(), false);

    let file = (0..OWNER_COUNT)
        .map(|index| format!("val x{index} = {VALID_OWNER}\n"))
        .collect::<String>();
    let (sources, source_id) = add_source(file);
    let parsed = parse_file_twice(&sources, source_id, "owner-rich file");
    assert!(parsed.diagnostics().is_empty());
    assert_eq!(parsed.roots().len(), OWNER_COUNT);
    assert!(parsed.roots().iter().all(|root| matches!(
        parsed.ast().items().get(*root),
        Ok(node) if matches!(node.payload(), Item::Variable { .. })
    )));
    assert_interpolation_nodes(parsed.ast(), false);
}

#[test]
fn every_public_entry_recovers_each_large_empty_interpolation_once() {
    let arguments = repeated(",", RECOVERED_OWNER);
    let (sources, source_id) = add_source(format!("call({arguments})"));
    let parsed = parse_expression_twice(&sources, source_id, "owner recovery expression");
    assert_diagnostic_series(parsed.diagnostics());
    assert_call_arguments(parsed.ast(), parsed.root());
    assert_interpolation_nodes(parsed.ast(), true);

    let (sources, source_id) = add_source(format!("val result = call({arguments})"));
    let parsed = parse_declaration_twice(&sources, source_id, "owner recovery declaration");
    assert_diagnostic_series(parsed.diagnostics());
    let Item::Variable { initializer, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("owner recovery declaration root")
        .payload()
    else {
        panic!("recovered declaration must remain a variable")
    };
    assert_call_arguments(parsed.ast(), *initializer);
    assert_interpolation_nodes(parsed.ast(), true);

    let body = local_variables(RECOVERED_OWNER);
    let (sources, source_id) = add_source(format!("{{\n{body}}}"));
    let parsed = parse_block_twice(&sources, source_id, "owner recovery block");
    assert_diagnostic_series(parsed.diagnostics());
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(parsed.root())
        .expect("owner recovery block root")
        .payload()
    else {
        panic!("recovered block must retain its root")
    };
    assert_eq!(elements.len(), OWNER_COUNT);
    assert!(elements.iter().all(|element| matches!(
        parsed.ast().statements().get(*element),
        Ok(node) if matches!(node.payload(), Statement::LocalVariable { .. })
    )));
    assert_interpolation_nodes(parsed.ast(), true);

    let file = (0..OWNER_COUNT)
        .map(|index| format!("val x{index} = {RECOVERED_OWNER}\n"))
        .collect::<String>();
    let (sources, source_id) = add_source(file);
    let parsed = parse_file_twice(&sources, source_id, "owner recovery file");
    assert_diagnostic_series(parsed.diagnostics());
    assert_eq!(parsed.roots().len(), OWNER_COUNT);
    assert!(parsed.roots().iter().all(|root| matches!(
        parsed.ast().items().get(*root),
        Ok(node) if matches!(node.payload(), Item::Variable { .. })
    )));
    assert_interpolation_nodes(parsed.ast(), true);
}
