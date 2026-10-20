//! Ordinary unresolved field types must not become raw ownership internal errors.
//! These sources use only the public memory-document facade and public observations.

use lang_frontend::{
    analysis::{UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names, analyze_unit_recovery},
    diagnostic::{Diagnostic, Severity},
    ownership_checking::CompilationUnitOwnership,
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, Copyability, TypeEnvironment, UnitTypeKind, standard_environments,
    },
};

fn memory(text: &str) -> (UnitNameSnapshot, TypeEnvironment) {
    let mut sources = SourceMap::new();
    let excluded = sources
        .add_source("excluded", "# absent")
        .expect("real excluded source");
    let source = sources
        .add_source("untitled:NominalError", text)
        .expect("ordinary selected source");
    assert_ne!(source, excluded);
    let (names, environment) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        names,
    )
    .expect("ordinary name prefix");
    assert_eq!(snapshot.descriptors().len(), 1);
    assert_eq!(snapshot.descriptors()[0].source_id(), source);
    assert_eq!(snapshot.descriptors()[0].root_identity(), None);
    assert_eq!(snapshot.descriptors()[0].logical_path(), None);
    assert_eq!(snapshot.parsed_files().len(), 1);
    assert_eq!(snapshot.parsed_files()[0].source_id(), source);
    assert!(snapshot.parsed_files()[0].diagnostics().is_empty());
    assert_eq!(snapshot.sources().source_text(source).unwrap(), text);
    assert_eq!(
        snapshot.sources().source_text(excluded).unwrap(),
        "# absent"
    );
    (snapshot, environment)
}

fn span_in(snapshot: &UnitNameSnapshot, context: &str, token: &str) -> Span {
    let source = snapshot.descriptors()[0].source_id();
    let text = snapshot.sources().source_text(source).unwrap();
    let contexts = text.match_indices(context).collect::<Vec<_>>();
    let tokens = context.match_indices(token).collect::<Vec<_>>();
    assert_eq!(contexts.len(), 1, "unique source context {context}");
    assert_eq!(tokens.len(), 1, "unique token within context {token}");
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

fn assert_execution_facts_absent(typed: &CompilationUnitTypes, owned: &CompilationUnitOwnership) {
    assert!(typed.sequential_iterations().is_empty());
    assert!(typed.map_iterations().is_empty());
    assert!(typed.ownership_primitives().is_empty());
    assert!(typed.integer_operations().is_empty());
    assert!(typed.range_sizes().is_empty());
    assert!(
        typed
            .calls()
            .iter()
            .all(|call| call.range_construction().is_none())
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.map_iterations().is_empty());
    assert!(owned.formed_owned_locals().is_empty());
    assert!(owned.loans().is_empty());
    assert!(owned.value_deliveries().is_empty());
    assert!(owned.receiver_facts().is_empty());
    assert!(owned.conditional_receiver_deliveries().is_empty());
    assert!(owned.construction_plans().is_empty());
    assert!(owned.non_null_assertions().is_empty());
    assert!(owned.ownership_primitives().is_empty());
    assert!(owned.field_replacements().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.conditional_receiver_drops().is_empty());
    assert!(typed.signatures().clone().validate().is_err());
    assert!(typed.clone().validate().is_err());
    assert!(typed.clone().validate_constants().is_err());
    assert!(owned.clone().validate().is_err());
}

fn observe(text: &str, independent_move: bool, calls: usize) {
    let (snapshot, environment) = memory(text);
    assert_diagnostics(
        snapshot.names().diagnostics(),
        &[("L0080", span_in(&snapshot, "Missing", "Missing"))],
    );
    assert!(snapshot.validated_names().is_none());
    let (typed, owned) = analyze_unit_recovery(&snapshot, &environment)
        .expect("a real unresolved field type must not become an internal ownership failure");
    assert!(typed.signatures().diagnostics().is_empty());
    assert!(typed.body_diagnostics().is_empty());
    assert!(typed.diagnostics().is_empty());
    let nominals = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|signature| signature.nominal())
        .collect::<Vec<_>>();
    assert_eq!(nominals.len(), 1);
    let nominal = nominals[0];
    assert!(matches!(
        typed.types().get(nominal.ty()),
        Some(UnitTypeKind::Nominal { .. })
    ));
    assert_eq!(nominal.fields().len(), 1);
    assert_eq!(
        typed.types().get(nominal.fields()[0].ty()),
        Some(&UnitTypeKind::Error)
    );
    assert_eq!(
        typed.copyability(nominal.ty()),
        Copyability::Error,
        "recovery must not reclassify the public capability"
    );
    let expected = if independent_move {
        vec![("L0133", span_in(&snapshot, "takeText(text)", "text"))]
    } else {
        Vec::new()
    };
    assert_diagnostics(owned.diagnostics(), &expected);
    assert!(owned.deferred().is_empty());
    assert_eq!(typed.calls().len(), calls);
    assert_eq!(owned.call_argument_contracts().len(), calls);
    assert_execution_facts_absent(&typed, &owned);
}

#[test]
fn nominal_field_error_preserves_echo_and_take_recovery() {
    // First independent analysis reaches consume_place from a real return.
    observe(
        "value class Broken(val field: Missing)\n\
         fun echo(own value: Broken): Broken = value",
        false,
        0,
    );
    // Second independent analysis also reaches value_delivery; the following
    // valid String call must still report its unrelated borrowed-move error.
    observe(
        "value class Broken(val field: Missing)\n\
         fun take(own item: Broken): Unit {}\n\
         fun takeText(own item: String): Unit {}\n\
         fun run(own value: Broken, text: String): Unit {\n\
           take(value)\n\
           takeText(text)\n\
         }",
        true,
        2,
    );
}
