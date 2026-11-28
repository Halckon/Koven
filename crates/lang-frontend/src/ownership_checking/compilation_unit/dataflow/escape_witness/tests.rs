//! Private metadata algebra, not a claim of public support for unknown sources.
use super::{Coverage, EscapeWitnesses};
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::type_checking::UnitExpressionId;

#[test]
fn unknown_branch_keeps_other_reachable_positive_evidence_at_join() {
    let fixture = Fixture::new(&[(
        "join.ko",
        "fun run(flag: Boolean) { val f: () -> Unit = {}; var optional = if (flag) f else null }",
    )]);
    let symbol = fixture.symbol(0, "optional");
    let source = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{}"));
    let mut known = EscapeWitnesses::default();
    known.mutable_locals.insert(symbol);
    known.known_bindings.insert(symbol);
    known.bindings.insert(symbol, [source].into());
    known.coverage.insert(source, Coverage::Known);
    let mut unknown = EscapeWitnesses::default();
    unknown.mutable_locals.insert(symbol);
    unknown.coverage.insert(source, Coverage::Unknown);
    for (mut left, right) in [(known.clone(), unknown.clone()), (unknown, known)] {
        left.merge(right);
        assert!(left.mutable_locals.contains(&symbol));
        assert!(!left.known_bindings.contains(&symbol));
        assert_eq!(left.bindings.get(&symbol), Some(&[source].into()));
        assert_eq!(left.coverage.get(&source), Some(&Coverage::Unknown));
    }
}

#[test]
fn disjoint_branch_expression_coverage_is_not_a_binding_intersection() {
    let fixture = Fixture::new(&[(
        "branches.ko",
        "fun run(flag: Boolean) { val one = {}; val two = {}; var optional = if (flag) one else two }",
    )]);
    let one = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "one"));
    let two = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "two"));
    let mut left = EscapeWitnesses::default();
    let mut right = EscapeWitnesses::default();
    left.coverage.insert(one, Coverage::Known);
    right.coverage.insert(two, Coverage::Known);
    left.merge(right);
    assert_eq!(left.coverage.get(&one), Some(&Coverage::Known));
    assert_eq!(left.coverage.get(&two), Some(&Coverage::Known));
}

#[test]
fn missing_syntactic_eligibility_is_not_unknown_value_coverage() {
    let fixture = Fixture::new(&[(
        "eligibility.ko",
        "fun run(flag:Boolean) { val f = {}; var optional=if(flag) f else null }",
    )]);
    let symbol = fixture.symbol(0, "optional");
    let source = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{}"));
    let mut eligible = EscapeWitnesses::default();
    eligible.mutable_locals.insert(symbol);
    eligible.known_bindings.insert(symbol);
    eligible.bindings.insert(symbol, [source].into());
    for (mut left, right) in [
        (eligible.clone(), EscapeWitnesses::default()),
        (EscapeWitnesses::default(), eligible),
    ] {
        left.merge(right);
        assert!(!left.mutable_locals.contains(&symbol));
        assert!(!left.known_bindings.contains(&symbol));
        assert!(!left.bindings.contains_key(&symbol));
    }
}

#[test]
fn non_callable_integer_assignments_do_not_populate_escape_coverage() {
    use super::super::{
        Checker, Codes, State, callable_provenance, closure, collect_construction_descriptors,
        traversal,
    };
    use crate::diagnostic::codes;
    use std::collections::BTreeMap;

    let text = "fun run(flag: Boolean) { var n = 0; n = 1; n = 2; n = 3; val f: () -> Int = { n }; val empty = if (flag) f else null }";
    let fixture = Fixture::new(&[("coverage-scope.ko", text)]);
    let shared = fixture.shared();
    assert!(
        shared.diagnostics().is_empty(),
        "{:?}",
        shared.diagnostics()
    );
    let parts = fixture.parts(&shared);
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
    let capture_index = closure::capture_index(&parts.capture.captures, fixture.typed.types());
    let checker = Checker::new(
        &fixture.sources,
        &fixture.parsed[0],
        fixture.source(0),
        fixture.names.names(),
        fixture.typed.types(),
        &parts.bindings,
        &fields,
        &parts.arguments,
        &parts.receivers,
        &capture_index,
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
    let mut state = State::default();
    for (expr_id, _) in checker.parsed.ast().expressions().iter() {
        checker
            .record_escape_coverage(expr_id, false, &mut state)
            .unwrap();
    }
    let lambda = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{ n }"));
    let null_lit = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "null"));
    let if_expr = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "if (flag) f else null"),
    );
    let f_ref = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "f"));
    assert_eq!(state.escape_witnesses.coverage.len(), 4);
    assert_eq!(
        state.escape_witnesses.coverage.get(&null_lit),
        Some(&Coverage::Known)
    );
    assert_eq!(
        state.escape_witnesses.coverage.get(&lambda),
        Some(&Coverage::Unknown)
    );
    assert!(state.escape_witnesses.coverage.contains_key(&if_expr));
    assert!(state.escape_witnesses.coverage.contains_key(&f_ref));
}
