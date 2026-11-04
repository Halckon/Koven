//! SPEC-0341: preserve automatic component rejection with independent Unit oracles.
//! The parent suite retains its original Single projection and fixture witnesses.

use super::*;
use lang_frontend::{
    diagnostic::{DiagnosticCodeCatalog, Severity},
    source::SourceId,
    type_checking::UnitAggregateProjectionKind,
};

fn observe(
    text: &str,
    inspect: impl Fn(
        &SourceMap,
        SourceId,
        &[Diagnostic],
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
    ),
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("structural.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, source, "component partial-move source");
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = environments();
    for input in [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "structural.ko", source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        inspect(&sources, source, owned.diagnostics(), typed.types(), &owned);
    }
}

fn expected_component(
    sources: &SourceMap,
    source: SourceId,
    primary: std::ops::Range<usize>,
    field: std::ops::Range<usize>,
) -> Diagnostic {
    let catalog = DiagnosticCodeCatalog::try_new(&["L0132"]).unwrap();
    let mut diagnostic = Diagnostic::new(
        sources,
        Severity::Error,
        catalog.resolve("L0132").unwrap(),
        "cannot move a non-Copyable component out of its owner",
        sources.span(source, primary.start, primary.end).unwrap(),
    )
    .unwrap();
    diagnostic
        .add_label(
            sources,
            sources.span(source, field.start, field.end).unwrap(),
            "non-Copyable component declared here",
        )
        .unwrap();
    diagnostic
}

#[test]
fn original_partial_move_fixture_keeps_both_errors_and_complete_component_label() {
    let text = include_str!("../fixtures/phase3/structural-fail/partial-move.ko");
    let sidecar = include_str!("../fixtures/phase3/structural-fail/partial-move.diag");
    observe(text, |sources, source, unit, typed, owned| {
        let diagnostics = unit;
        let actual = diagnostics
            .iter()
            .map(|diagnostic| {
                let span = diagnostic.primary_span();
                assert_eq!(span.source_id(), source);
                format!("{}\t{}\t{}", diagnostic.code(), span.start(), span.end())
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(actual, sidecar);
        let field = text.find("first").unwrap();
        let expected = expected_component(sources, source, 214..224, field..field + 5);
        assert_eq!(diagnostics[1], expected);
        let components = typed
            .aggregate_projections()
            .iter()
            .filter(|fact| fact.kind() == UnitAggregateProjectionKind::StructuralComponent)
            .collect::<Vec<_>>();
        assert_eq!(components.len(), 1);
        assert_eq!(typed.copyability(components[0].ty()), Copyability::MoveOnly);
        let call = typed.call(components[0].expression()).unwrap();
        assert_eq!(
            call.instance().target(),
            lang_frontend::type_checking::UnitCallTarget::StructuralComponent(
                components[0].field()
            )
        );
        assert!(
            owned.drops().is_empty(),
            "invalid source cannot publish a drop plan"
        );
    });
}

#[test]
fn borrowed_owner_still_cannot_return_an_owned_automatic_component() {
    let text = "class Resource {}\nvalue class Pair(val first: Resource)\nfun fail(pair: Pair): Unit { val value = pair.component1() }";
    observe(text, |sources, source, unit, _, owned| {
        let primary = text.find("component1").unwrap();
        let field = text.find("first").unwrap();
        let expected = [expected_component(
            sources,
            source,
            primary..primary + 10,
            field..field + 5,
        )];
        assert_eq!(unit, expected);
        assert!(owned.drops().is_empty());
    });
}

#[test]
fn copyable_component_and_explicit_owned_result_are_not_partial_moves() {
    let text = "class Resource {}\nvalue class Pair(val first: Resource, val second: Int)\nvalue class Custom(val first: Resource) { fun component1(): Resource = Resource() }\nfun pass(own pair: Pair, own custom: Custom): Unit { val copied = pair.component2(); val created = custom.component1() }";
    observe(text, |_, _, unit, typed, _| {
        assert!(unit.is_empty(), "{unit:?}");
        let components = typed
            .aggregate_projections()
            .iter()
            .filter(|fact| fact.kind() == UnitAggregateProjectionKind::StructuralComponent)
            .collect::<Vec<_>>();
        assert_eq!(
            components.len(),
            1,
            "explicit method is not an automatic projection"
        );
        assert_eq!(typed.copyability(components[0].ty()), Copyability::Copyable);
    });
}

#[test]
fn receiver_use_after_move_precedes_component_rejection_without_a_cascade() {
    let text = "class Resource {}\nvalue class Pair(val first: Resource)\nfun take(own value: Pair): Unit {}\nfun fail(own pair: Pair): Unit { take(pair); val value = pair.component1() }";
    observe(text, |sources, _, unit, _, owned| {
        let diagnostics = unit;
        assert_eq!(codes(diagnostics), ["L0131"]);
        assert_eq!(sources.slice(diagnostics[0].primary_span()), Ok("pair"));
        assert!(owned.drops().is_empty());
    });
}
