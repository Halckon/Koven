//! 已拒绝 extension 的恢复上下文不得泄漏到同源码后续无 receiver 函数。

use lang_frontend::{
    analysis::{UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names, analyze_unit_recovery},
    diagnostic::{Diagnostic, Severity},
    parser::Expression,
    source::{SourceMap, Span},
    type_checking::{TypeEnvironment, UnitExpressionId, UnitTypeKind, standard_environments},
};

fn memory(text: &str) -> (UnitNameSnapshot, TypeEnvironment) {
    let mut sources = SourceMap::new();
    let prior = sources
        .add_source("unselected", "# absent")
        .expect("real prior registration");
    let source = sources
        .add_source("untitled:ThisContext", text)
        .expect("real selected source");
    assert_ne!(source, prior);
    let (names, environment) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        names,
    )
    .expect("ordinary parsed memory input");
    assert_eq!(snapshot.descriptors().len(), 1);
    assert_eq!(snapshot.descriptors()[0].source_id(), source);
    assert_eq!(snapshot.descriptors()[0].root_identity(), None);
    assert_eq!(snapshot.descriptors()[0].logical_path(), None);
    assert_eq!(snapshot.parsed_files().len(), 1);
    assert_eq!(snapshot.parsed_files()[0].source_id(), source);
    assert!(snapshot.parsed_files()[0].diagnostics().is_empty());
    assert_eq!(snapshot.sources().source_text(source).unwrap(), text);
    assert_eq!(snapshot.sources().source_text(prior).unwrap(), "# absent");
    (snapshot, environment)
}

fn span_in(snapshot: &UnitNameSnapshot, context: &str, token: &str) -> Span {
    let source = snapshot.descriptors()[0].source_id();
    let text = snapshot.sources().source_text(source).unwrap();
    let contexts = text.match_indices(context).collect::<Vec<_>>();
    let tokens = context.match_indices(token).collect::<Vec<_>>();
    assert_eq!(
        contexts.len(),
        1,
        "unique expected source context {context}"
    );
    assert_eq!(
        tokens.len(),
        1,
        "unique expected token within context {token}"
    );
    let start = contexts[0].0 + tokens[0].0;
    snapshot
        .sources()
        .span(source, start, start + token.len())
        .unwrap()
}

fn assert_diagnostics(diagnostics: &[Diagnostic], expected: &[(&str, Span)]) {
    assert_eq!(diagnostics.len(), expected.len(), "{diagnostics:?}");
    for (diagnostic, (code, span)) in diagnostics.iter().zip(expected) {
        assert_eq!(diagnostic.code().to_string(), *code);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.primary_span(), *span);
    }
}

fn two_this_expressions(snapshot: &UnitNameSnapshot, expected: [Span; 2]) -> [UnitExpressionId; 2] {
    let file = &snapshot.parsed_files()[0];
    let expressions = file
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::This))
        .collect::<Vec<_>>();
    assert_eq!(
        expressions.len(),
        2,
        "two actual parser-produced this expressions"
    );
    let unit = snapshot
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == file.source_id())
        .expect("original selected source");
    let ids = expected.map(|span| {
        let matching = expressions
            .iter()
            .filter(|(_, node)| node.span() == span)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "actual this at the expected source span");
        UnitExpressionId::new(unit.id(), matching[0])
    });
    assert_ne!(
        ids[0], ids[1],
        "distinct real expressions in the same source unit"
    );
    ids
}

#[test]
fn rejected_extension_context_does_not_suppress_l0166_in_the_next_top_level_function() {
    let (snapshot, environment) = memory(
        "borrow fun List<Int>.probe(): Unit { this }\n\
         fun broken(): Unit { this }",
    );
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
    let extension_dot = span_in(&snapshot, "List<Int>.probe", ".");
    let extension_this = span_in(&snapshot, "probe(): Unit { this }", "this");
    let broken_this = span_in(&snapshot, "broken(): Unit { this }", "this");
    let expressions = two_this_expressions(&snapshot, [extension_this, broken_this]);

    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
        .expect("both ordinary source errors recover without an internal failure");
    assert_diagnostics(
        typed.signatures().diagnostics(),
        &[("L0164", extension_dot)],
    );
    assert_diagnostics(typed.body_diagnostics(), &[("L0166", broken_this)]);
    assert_diagnostics(
        typed.diagnostics(),
        &[("L0164", extension_dot), ("L0166", broken_this)],
    );
    let callables = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.callable())
        .collect::<Vec<_>>();
    assert_eq!(callables.len(), 2);
    assert_eq!(
        callables
            .iter()
            .filter(|callable| callable.receiver().is_some())
            .count(),
        1
    );
    assert!(
        callables
            .iter()
            .all(|callable| callable.range_extension().is_none())
    );
    for expression in expressions {
        let ty = typed
            .expression_type(expression)
            .expect("real this type fact");
        assert_eq!(typed.types().get(ty), Some(&UnitTypeKind::Error));
    }
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(typed.clone().validate().is_err());
    assert!(typed.clone().validate_constants().is_err());
    assert!(owned.clone().validate().is_err());
    assert!(owned.drops().is_empty());
    assert!(owned.iterations().is_empty());
    assert!(owned.map_iterations().is_empty());
}
