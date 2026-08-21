//! SPEC-0151 的四公开入口大平坦列表与重复错误恢复压力矩阵。

use lang_frontend::{
    diagnostic::Diagnostic,
    parser::{Expression, Item, Statement},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    parse_block_twice, parse_declaration_twice, parse_expression_twice, parse_file_twice,
};

const ELEMENT_COUNT: usize = 4_096;

fn add_source(source: &str) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-stress.ko", source)
        .expect("stress source name must be unique");
    (sources, source_id)
}

fn assert_diagnostic_series(diagnostics: &[Diagnostic], code: &str) {
    assert_eq!(
        diagnostics.len(),
        ELEMENT_COUNT,
        "expected {code}, got {:?}",
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
            .all(|diagnostic| diagnostic.code().to_string() == code)
    );
    assert!(
        diagnostics
            .windows(2)
            .all(|window| { window[0].primary_span().start() < window[1].primary_span().start() })
    );
}

#[test]
fn every_public_entry_preserves_each_large_flat_legal_element() {
    let arguments = (0..ELEMENT_COUNT)
        .map(|index| format!("x{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let expression_source = format!("call({arguments})");
    let (sources, source_id) = add_source(&expression_source);
    let parsed = parse_expression_twice(&sources, source_id, "large call argument list");
    assert!(parsed.diagnostics().is_empty());
    let Expression::Call { arguments, .. } = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .expect("expression root")
        .payload()
    else {
        panic!("large expression root must remain a call")
    };
    assert_eq!(arguments.len(), ELEMENT_COUNT);

    let parameters = (0..ELEMENT_COUNT)
        .map(|index| format!("p{index}: T"))
        .collect::<Vec<_>>()
        .join(",");
    let declaration_source = format!("fun stress({parameters}): Unit");
    let (sources, source_id) = add_source(&declaration_source);
    let parsed = parse_declaration_twice(&sources, source_id, "large value parameter list");
    assert!(parsed.diagnostics().is_empty());
    let Item::Function { parameters, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("declaration root")
        .payload()
    else {
        panic!("large declaration root must remain a function")
    };
    assert_eq!(parameters.len(), ELEMENT_COUNT);

    let block_body = (0..ELEMENT_COUNT)
        .map(|index| format!("val x{index} = 0\n"))
        .collect::<String>();
    let block_source = format!("{{\n{block_body}}}");
    let (sources, source_id) = add_source(&block_source);
    let parsed = parse_block_twice(&sources, source_id, "large block element list");
    assert!(parsed.diagnostics().is_empty());
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(parsed.root())
        .expect("block root")
        .payload()
    else {
        panic!("large block root must remain a block")
    };
    assert_eq!(elements.len(), ELEMENT_COUNT);

    let file_source = (0..ELEMENT_COUNT)
        .map(|index| format!("val x{index} = 0\n"))
        .collect::<String>();
    let (sources, source_id) = add_source(&file_source);
    let parsed = parse_file_twice(&sources, source_id, "large file root list");
    assert!(parsed.diagnostics().is_empty());
    assert_eq!(parsed.roots().len(), ELEMENT_COUNT);
}

#[test]
fn every_public_entry_recovers_each_large_flat_error_region_once() {
    let expression_source = format!("{}z", "a++".repeat(ELEMENT_COUNT));
    let (sources, source_id) = add_source(&expression_source);
    let parsed = parse_expression_twice(&sources, source_id, "large unsupported suffix list");
    assert_diagnostic_series(parsed.diagnostics(), "L0015");

    let declaration_source = format!("fun stress({}tail: T): Unit", ",".repeat(ELEMENT_COUNT));
    let (sources, source_id) = add_source(&declaration_source);
    let parsed = parse_declaration_twice(&sources, source_id, "large empty parameter list");
    assert_diagnostic_series(parsed.diagnostics(), "L0024");
    let Item::Function { parameters, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("declaration error root")
        .payload()
    else {
        panic!("recovered declaration must remain a function")
    };
    assert_eq!(parameters.len(), 1);

    let block_source = format!("{{ {} }}", "@ ".repeat(ELEMENT_COUNT));
    let (sources, source_id) = add_source(&block_source);
    let parsed = parse_block_twice(&sources, source_id, "large block error list");
    assert_diagnostic_series(parsed.diagnostics(), "L0029");
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(parsed.root())
        .expect("block error root")
        .payload()
    else {
        panic!("recovered block must retain its root")
    };
    assert_eq!(elements.len(), ELEMENT_COUNT);
    assert!(elements.iter().all(|element| {
        matches!(
            parsed.ast().statements().get(*element),
            Ok(node) if matches!(node.payload(), Statement::Error)
        )
    }));

    let file_source = (0..ELEMENT_COUNT)
        .map(|index| format!("@\nval sentinel{index} = 0\n"))
        .collect::<String>();
    let (sources, source_id) = add_source(&file_source);
    let parsed = parse_file_twice(&sources, source_id, "large file error list");
    assert_diagnostic_series(parsed.diagnostics(), "L0017");
    assert_eq!(parsed.roots().len(), ELEMENT_COUNT * 2);
    assert!(parsed.roots().chunks_exact(2).all(|roots| {
        matches!(
            parsed.ast().items().get(roots[0]),
            Ok(node) if matches!(node.payload(), Item::Error)
        ) && matches!(
            parsed.ast().items().get(roots[1]),
            Ok(node) if matches!(node.payload(), Item::Variable { .. })
        )
    }));
}
