//! Unit ownership oracles for real field moves and explicitly declared component methods.

use super::*;
use lang_frontend::{
    diagnostic::{DiagnosticCodeCatalog, Severity},
    source::SourceId,
    type_checking::UnitCallTarget,
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
    parameter: std::ops::Range<usize>,
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
        .add_label(
            sources,
            sources
                .span(source, parameter.start, parameter.end)
                .unwrap(),
            "selected parameter declared here",
        )
        .unwrap();
    diagnostic
}

#[test]
fn partial_move_fixture_keeps_real_field_diagnostic_and_label() {
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
        let parameter = "own item: Resource";
        let parameter_start = text.find(parameter).unwrap();
        let expected = expected_component(
            sources,
            source,
            182..187,
            field..field + 5,
            parameter_start..parameter_start + parameter.len(),
        );
        assert_eq!(diagnostics, [expected]);
        let [projection] = typed.aggregate_projections() else {
            panic!("only the real field is projected");
        };
        assert_eq!(typed.copyability(projection.ty()), Copyability::MoveOnly);
        assert!(
            owned.drops().is_empty(),
            "invalid source cannot publish a drop plan"
        );
    });
}

#[test]
fn explicitly_declared_component_methods_use_normal_call_targets() {
    let text = "class Resource {}
        value class Value(val first: Resource) { fun component1(): Resource = Resource() }
        class Holder(val amount: Int) { fun component1(): Int = this.amount }
        fun read(value: Value, holder: Holder): Int {
            val created = value.component1()
            return holder.component1()
        }";
    observe(text, |_, _, diagnostics, typed, _| {
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            typed
                .calls()
                .iter()
                .filter(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
                .count(),
            2
        );
        assert_eq!(typed.aggregate_projections().len(), 1, "only this.amount");
    });
    let (_, _, names, typed, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(
        typed.calls().iter().filter(|call| matches!(call.target(), CallableTarget::Source(symbol) if names.symbols()[symbol.index()].name() == "component1")).count(),
        2
    );
    assert_eq!(typed.aggregate_projections().len(), 1, "only this.amount");
}
