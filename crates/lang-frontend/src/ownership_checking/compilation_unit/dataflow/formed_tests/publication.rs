//! CI-only original-source and rejecting-source guards; never execute mutations locally.
//! Stage A retains the legacy witness: this does not inject reversed/duplicate history.

use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        CompilationUnitOwnership, OwnershipDeferredReason, UnitDropTarget, UnitValueDeliveryKind,
        check_compilation_unit_ownership_recovering,
    },
    parser::parse_file,
    type_checking::{
        FinalTypeAdmissionError, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

const SOURCE: &str = r#"
fun identity(own item: String): String = item
fun run(): Unit {
    val seed = "kept"
    var source = identity(seed)
    val delivered = identity(source)
    var counter = 0
    counter = 1
}
fun dead(): Unit { return; val deadTail = "dead" }
fun stop(): Nothing = stop()
fun abort(): Unit { val stopped = stop(); val abortTail = "dead" }
"#;

fn expected_locals(fixture: &Fixture) -> Vec<UnitSymbolId> {
    let mut expected = ["seed", "source", "delivered", "counter"]
        .map(|name| fixture.local(0, name))
        .to_vec();
    expected.sort();
    assert!(expected.windows(2).all(|pair| pair[0] < pair[1]));
    expected
}

// Use production inputs and finish the full run before inspecting success or rejection.
fn with_source_checker<T>(
    fixture: &Fixture,
    parts: &Parts,
    inspect: impl FnOnce(&mut Checker<'_>) -> T,
) -> T {
    let inputs = fixture.inputs();
    let fields = traversal::collect_field_mutability(&inputs, fixture.names.names()).unwrap();
    let constructions = collect_construction_descriptors(
        fixture.typed.types().constructions(),
        fixture.names.names().index().source_units().len(),
    )
    .unwrap();
    let empty = BTreeMap::new();
    let catalog = codes::catalog().unwrap();
    let codes = Codes {
        use_after_move: catalog.resolve(codes::USE_AFTER_MOVE).unwrap(),
        partial_move: catalog.resolve(codes::PARTIAL_MOVE).unwrap(),
        borrowed_move: catalog.resolve(codes::MOVE_FROM_BORROWED_BINDING).unwrap(),
        loan_conflict: catalog.resolve(codes::LOAN_CONFLICT).unwrap(),
        immutable_inout: catalog.resolve(codes::IMMUTABLE_INOUT_PLACE).unwrap(),
        container_element_move: catalog.resolve(codes::MOVE_FROM_CONTAINER_ELEMENT).unwrap(),
        borrowed_closure_escape: catalog.resolve(codes::BORROWED_CLOSURE_ESCAPE).unwrap(),
        illegal_owned_capture: catalog.resolve(codes::ILLEGAL_OWNED_CAPTURE).unwrap(),
        non_transferable_delivery: catalog.resolve(codes::NON_TRANSFERABLE_DELIVERY).unwrap(),
    };
    let mut diagnostics = Vec::new();
    let mut loans = Vec::new();
    let mut deliveries = Vec::new();
    let mut receivers = Vec::new();
    let mut conditional = Vec::new();
    let mut rc = Vec::new();
    let mut construction_plans = Vec::new();
    let mut assertions = Vec::new();
    let mut callable_sources = callable_provenance::Collection::default();
    let mut checker = Checker::new(
        &fixture.sources,
        &fixture.parsed[0],
        fixture.source(0),
        fixture.names.names(),
        fixture.typed.types(),
        &parts.bindings,
        &fields,
        &parts.arguments,
        &parts.receivers,
        &parts.capture.captures,
        &parts.capture.closures,
        &parts.capture.transferabilities,
        constructions.get(&fixture.source(0)).unwrap_or(&empty),
        codes,
        &mut diagnostics,
        &mut loans,
        &mut deliveries,
        &mut receivers,
        &mut conditional,
        &mut rc,
        &mut construction_plans,
        &mut assertions,
        &mut callable_sources,
    )
    .unwrap();
    checker.range_producers = crate::ownership_checking::range_producer::collect_unit(
        &fixture.sources,
        &inputs,
        fixture.names.names(),
        fixture.typed.types(),
    )
    .unwrap();
    let drop_analysis = checker.run().unwrap();
    assert!(drop_analysis.deferred.is_empty());
    assert!(checker.primitive_deferred.is_empty());
    inspect(&mut checker)
}

#[test]
fn original_source_history_retains_moved_locals_and_excludes_unfinished_bindings() {
    let fixture = Fixture::new(&[("single.ko", SOURCE)]);
    let baseline = fixture.shared();
    assert!(
        baseline.diagnostics().is_empty(),
        "{:?}",
        baseline.diagnostics()
    );
    assert!(baseline.deferred().is_empty(), "{:?}", baseline.deferred());
    baseline.clone().validate().unwrap();
    let expected = expected_locals(&fixture);
    assert_eq!(baseline.formed_owned_locals(), expected);
    for excluded in ["deadTail", "stopped", "abortTail"] {
        assert!(!expected.contains(&fixture.local(0, excluded)));
    }
    for name in ["seed", "source"] {
        let symbol = fixture.local(0, name);
        assert!(
            baseline.value_deliveries().iter().any(|fact| {
                fact.kind() == UnitValueDeliveryKind::Move
                    && fact.place().is_some_and(|place| place.root() == symbol)
            }),
            "history retains actual moved source: {name}"
        );
        assert!(baseline.drops().iter().all(|drop| {
            !matches!(drop.target(), UnitDropTarget::Named(owner) if owner == symbol)
        }));
    }
    let parts = fixture.parts(&baseline);
    let raw = fixture.dataflow(&parts);
    assert!(raw.diagnostics.is_empty());
    assert!(raw.deferred.is_empty());
    assert_eq!(raw.formed_owned_locals, expected);
}

#[test]
fn original_source_history_final_gate_preserves_each_rejection_combination() {
    let fixture = Fixture::new(&[("single.ko", SOURCE)]);
    let baseline = fixture.shared();
    assert!(baseline.diagnostics().is_empty());
    assert!(baseline.deferred().is_empty());
    baseline.clone().validate().unwrap();
    let expected = expected_locals(&fixture);
    assert_eq!(baseline.formed_owned_locals(), expected);
    let parts = fixture.parts(&baseline);
    let diagnostic = with_source_checker(&fixture, &parts, |checker| {
        assert!(checker.diagnostics.is_empty());
        let before = Facts::read(checker);
        let mut formed = before.formed.clone();
        formed.sort();
        formed.dedup();
        assert_eq!(formed, expected);
        let counter = fixture.local(0, "counter");
        // run() recollects variable mutability, so mutate after the successful run.
        // This checks the real assignment guard, not a full rejecting Checker::run().
        let original_kinds = checker.variable_kinds.clone();
        assert_eq!(
            checker.variable_kinds.insert(counter, VariableKind::Val),
            Some(VariableKind::Var)
        );
        assert_eq!(checker.variable_kinds.len(), original_kinds.len());
        assert_eq!(
            checker
                .variable_kinds
                .iter()
                .filter(|(symbol, kind)| { original_kinds.get(*symbol) != Some(*kind) })
                .count(),
            1
        );
        let assignment = fixture.expression(0, "counter = 1");
        let Expression::Assignment {
            target,
            operator,
            value,
            ..
        } = *fixture.parsed[0]
            .ast()
            .expressions()
            .get(assignment)
            .unwrap()
            .payload()
        else {
            panic!("original counter assignment");
        };
        checker
            .check_assignment(assignment, target, operator, value, State::default())
            .unwrap();
        assert_eq!(checker.diagnostics.len(), 1);
        let primary = fixture.parsed[0]
            .ast()
            .expressions()
            .get(target)
            .unwrap()
            .span();
        assert_eq!(fixture.sources.slice(primary).unwrap(), "counter");
        assert_eq!(primary.source_id(), fixture.parsed[0].source_id());
        let mut expected_diagnostic = Diagnostic::new(
            &fixture.sources,
            Severity::Error,
            checker.codes.immutable_inout,
            "assignment target is not a mutable place",
            primary,
        )
        .unwrap();
        expected_diagnostic
            .add_label(
                &fixture.sources,
                checker.symbol_span(counter).unwrap(),
                "immutable binding declared here",
            )
            .unwrap();
        assert_eq!(checker.diagnostics[0].code().to_string(), "L0134");
        assert_eq!(checker.diagnostics[0], expected_diagnostic);
        assert_eq!(
            Facts::read(checker),
            before,
            "all nine fact classes remain intact"
        );
        assert!(checker.primitive_deferred.is_empty());
        checker.diagnostics[0].clone()
    });
    let expression =
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "identity(source)"));
    let deferred =
        UnitOwnershipDeferredFact::new(expression, OwnershipDeferredReason::MemberReceiver);
    for (has_diagnostic, has_deferred) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let parts = fixture.parts(&baseline);
        let mut raw = fixture.dataflow(&parts);
        assert!(raw.diagnostics.is_empty());
        assert!(raw.deferred.is_empty());
        assert_eq!(
            raw.formed_owned_locals, expected,
            "nonempty final gate input"
        );
        let diagnostics = if has_diagnostic {
            vec![diagnostic.clone()]
        } else {
            Vec::new()
        };
        let deferred = if has_deferred {
            vec![deferred]
        } else {
            Vec::new()
        };
        raw.diagnostics = diagnostics.clone();
        raw.deferred = deferred.clone();
        // Final construction only gates publication; sorting/deduplication happens
        // upstream. The legacy reversed/duplicate-input witness must remain.
        let product = CompilationUnitOwnership::new(
            fixture.typed.types(),
            parts.bindings.into_values().collect(),
            parts.arguments,
            parts.receivers,
            parts.capture,
            raw,
        );
        assert_eq!(product.diagnostics(), diagnostics);
        assert_eq!(product.deferred(), deferred);
        assert!(product.is_compatible_with(&fixture.typed));
        if has_diagnostic || has_deferred {
            assert!(product.formed_owned_locals().is_empty());
            assert!(product.validate().is_err());
        } else {
            assert_eq!(product.formed_owned_locals(), expected);
            product.validate().unwrap();
        }
    }
}

#[test]
fn immutable_counter_source_rejects_full_unit_run_and_clears_published_history() {
    // This is an additional real rejecting source, not the original-source mutation witness.
    // Unit run() recollects mutability, so preloading a Val over the original Var is not valid.
    assert_eq!(SOURCE.matches("var counter = 0").count(), 1);
    let rejected_source = SOURCE.replacen("var counter = 0", "val counter = 0", 1);
    let fixture = Fixture::new(&[("immutable_counter.ko", &rejected_source)]);
    let expected_history =
        ["seed", "source", "delivered", "counter"].map(|name| fixture.local(0, name));
    let counter = fixture.local(0, "counter");
    let assignment = fixture.expression(0, "counter = 1");
    let Expression::Assignment { target, .. } = fixture.parsed[0]
        .ast()
        .expressions()
        .get(assignment)
        .unwrap()
        .payload()
    else {
        panic!("real immutable counter assignment");
    };
    let primary = fixture.parsed[0]
        .ast()
        .expressions()
        .get(*target)
        .unwrap()
        .span();
    let declaration = fixture.names.names().source_units()[counter.source_unit().index()]
        .resolution()
        .symbols()[counter.symbol().index()]
    .span();
    assert_eq!(fixture.sources.slice(primary).unwrap(), "counter");
    assert_eq!(fixture.sources.slice(declaration).unwrap(), "counter");
    assert_eq!(primary.source_id(), fixture.parsed[0].source_id());
    assert_eq!(declaration.source_id(), primary.source_id());
    let mut expected = Diagnostic::new(
        &fixture.sources,
        Severity::Error,
        codes::catalog()
            .unwrap()
            .resolve(codes::IMMUTABLE_INOUT_PLACE)
            .unwrap(),
        "assignment target is not a mutable place",
        primary,
    )
    .unwrap();
    expected
        .add_label(
            &fixture.sources,
            declaration,
            "immutable binding declared here",
        )
        .unwrap();
    assert_eq!(expected.code().to_string(), "L0134");

    let shared = fixture.shared();
    assert_eq!(shared.diagnostics(), [expected.clone()]);
    assert!(shared.deferred().is_empty());
    let parts = fixture.parts(&shared);
    with_source_checker(&fixture, &parts, |checker| {
        assert_eq!(*checker.diagnostics, [expected.clone()]);
        assert_eq!(
            checker.variable_kinds.get(&counter),
            Some(&VariableKind::Val)
        );
        assert_eq!(
            checker.formed_owned_locals, expected_history,
            "the full rejecting run has nonempty history before the aggregate gate"
        );
        assert!(checker.primitive_deferred.is_empty());
    });
    let raw = fixture.dataflow(&parts);
    assert_eq!(raw.diagnostics, [expected.clone()]);
    assert!(raw.deferred.is_empty());
    assert!(raw.formed_owned_locals.is_empty());
    assert_eq!(shared.diagnostics(), [expected]);
    assert!(shared.formed_owned_locals().is_empty());
    assert!(shared.loans().is_empty());
    assert!(shared.value_deliveries().is_empty());
    assert!(shared.drops().is_empty());
    assert!(shared.is_compatible_with(&fixture.typed));
    assert!(shared.validate().is_err());
}

#[test]
fn original_typed_error_preserves_diagnostics_and_blocks_history_publication() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "typed_error.ko",
            format!("{SOURCE}\nfun wrong(): Int = true\n"),
        )
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let primaries = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            (sources.slice(node.span()).unwrap() == "true").then_some(node.span())
        })
        .collect::<Vec<_>>();
    let [primary] = primaries.as_slice() else {
        panic!("unique original true literal")
    };
    let labels = parsed
        .ast()
        .type_refs()
        .iter()
        .filter_map(|(_, node)| {
            (sources.slice(node.span()).unwrap() == "Int").then_some(node.span())
        })
        .collect::<Vec<_>>();
    let [label] = labels.as_slice() else {
        panic!("unique wrong return type")
    };
    let mut expected = Diagnostic::new(
        &sources,
        Severity::Error,
        codes::catalog()
            .unwrap()
            .resolve(codes::TYPE_MISMATCH)
            .unwrap(),
        "expression type does not match the expected type",
        *primary,
    )
    .unwrap();
    expected
        .add_label(&sources, *label, "expected Int, found Boolean")
        .unwrap();
    assert_eq!(expected.code().to_string(), "L0084");
    assert_eq!(primary.source_id(), source);
    for input in [
        SourceUnitInput::new("root", "typed_error.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (ne, te) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert_eq!(typed.diagnostics(), [expected.clone()]);
        let original = typed.diagnostics().to_vec();
        let Err(FinalTypeAdmissionError::Diagnostics(rejected)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &te, &typed)
        else {
            panic!("real type mismatch must preserve diagnostics, never Internal or success");
        };
        assert_eq!(rejected, original);
        assert!(typed.clone().validate().is_err());
        let product = check_compilation_unit_ownership_recovering(
            &sources,
            &inputs,
            names.names(),
            &te,
            &typed,
        )
        .unwrap();
        assert!(product.diagnostics().is_empty(), "isolate the typed gate");
        assert!(product.deferred().is_empty());
        assert!(product.formed_owned_locals().is_empty());
        assert!(product.validate().is_err());
        assert_eq!(typed.diagnostics(), original);
    }
}
