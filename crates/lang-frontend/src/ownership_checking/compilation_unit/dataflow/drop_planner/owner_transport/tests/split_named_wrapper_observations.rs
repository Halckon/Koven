//! Observation checkpoint only: no multi-source capability, binding or lowering permission.
use super::super::origin_probe::{Event, WhileRole};
use super::*;
use crate::{ownership_checking::LoanKind, type_checking::UnitCallTarget};

const BEFORE: &str = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 };
println(a); println(b) }"#;
const WRAPPER: &str = "fun entry() { val p = 1; val c = 2; run(p, c) }";

#[test]
fn split_sources_preserve_actual_planner_traces_and_canonical_owned_order() {
    let after = BEFORE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "}; outer -= 1",
            "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
        );
    for (callee_text, child_first) in [(BEFORE, false), (after.as_str(), true)] {
        for callee_first in [true, false] {
            for reversed_inputs in [false, true] {
                let callee_path = if callee_first { "a.ko" } else { "z.ko" };
                let wrapper_path = if callee_first { "z.ko" } else { "a.ko" };
                let mut texts = [(callee_path, callee_text), (wrapper_path, WRAPPER)];
                if reversed_inputs {
                    texts.reverse();
                }
                let callee_index = usize::from(reversed_inputs);
                let wrapper_index = 1 - callee_index;
                let fixture = Fixture::new(&texts);
                let owned = fixture.shared();
                assert!(owned.diagnostics().is_empty());
                assert!(owned.deferred().is_empty());
                let parts = fixture.parts(&owned);
                let mut traces: [Vec<Event>; 2] = [Vec::new(), Vec::new()];
                for (source_index, trace) in traces.iter_mut().enumerate() {
                    inspect_source(&fixture, &parts, source_index, |checker| {
                        assert_eq!(checker.parsed.roots().len(), 1);
                        let make = || {
                            super::super::super::DropPlanner::new(
                                checker,
                                super::super::super::liveness::build(checker, true).unwrap(),
                            )
                        };
                        let baseline = make().run().unwrap();
                        let mut planner = make();
                        // Observe only the real run; no fabricated events or candidate admission.
                        planner.origin_probe = Some(Vec::new());
                        planner.root_trace_ranges = Some(Vec::new());
                        let observed = planner.run().unwrap();
                        assert_eq!(observed.facts, baseline.facts);
                        assert_eq!(
                            observed.direct_call_loan_ends(),
                            baseline.direct_call_loan_ends()
                        );
                        assert_eq!(observed.owner_observations, baseline.owner_observations);
                        assert!(observed.wrapped_dual_pending().is_none());
                        assert!(observed.wrapped_after_child_pending().is_none());
                        let events = observed.origin_probe.as_ref().unwrap();
                        let ranges = observed.root_trace_ranges.as_ref().unwrap();
                        let [(root, range)] = ranges.as_slice() else {
                            panic!("one actual root range")
                        };
                        assert_eq!(root.source_unit(), checker.source_unit);
                        assert_eq!(root.item(), checker.parsed.roots()[0]);
                        assert_eq!(*range, 0..events.len());
                        *trace = events.clone();
                    });
                }
                let callee = &traces[callee_index];
                let wrapper = &traces[wrapper_index];
                assert_eq!(callee.len(), 34);
                assert_eq!(wrapper.len(), 5);
                let mut expected = vec![
                    "literal",
                    "bound",
                    "literal",
                    "bound",
                    "enter",
                    "condition",
                    "body",
                ];
                let replace = ["borrow", "literal", "replace", "removed", "bound", "drop"];
                if !child_first {
                    expected.extend(replace);
                }
                expected.extend(["enter", "condition", "body"]);
                expected.extend(replace);
                expected.extend(["body_exit", "exit"]);
                if child_first {
                    expected.extend(replace);
                }
                expected.extend([
                    "body_exit",
                    "exit",
                    "call",
                    "borrow",
                    "removed",
                    "drop",
                    "call",
                    "borrow",
                    "removed",
                    "drop",
                ]);
                assert_eq!(callee.iter().map(role).collect::<Vec<_>>(), expected);
                let [
                    Event::UncoveredCall(call),
                    Event::BorrowRegistered(p),
                    Event::BorrowRegistered(c),
                    Event::DirectBorrowEnded {
                        loan: c_end,
                        point: c_point,
                    },
                    Event::DirectBorrowEnded {
                        loan: p_end,
                        point: p_point,
                    },
                ] = wrapper.as_slice()
                else {
                    panic!("complete actual wrapper trace")
                };
                assert_eq!(call.source_unit(), fixture.source(wrapper_index));
                assert_eq!(
                    call.expression(),
                    fixture.expression(wrapper_index, "run(p, c)")
                );
                let UnitCallTarget::Declaration(target) =
                    fixture.typed.types().call(*call).unwrap().target()
                else {
                    panic!("actual cross-source declaration")
                };
                let declaration = fixture
                    .names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.id() == target)
                    .unwrap();
                assert_eq!(declaration.source_unit(), fixture.source(callee_index));
                assert_eq!(declaration.root(), fixture.parsed[callee_index].roots()[0]);
                assert_ne!(declaration.source_unit(), call.source_unit());
                for (loan, argument) in [(p, "p"), (c, "c")] {
                    assert_eq!(loan.call(), *call);
                    assert_eq!(loan.kind(), LoanKind::Shared);
                    assert_eq!(loan.argument().source_unit(), fixture.source(wrapper_index));
                    assert_eq!(
                        loan.argument().expression(),
                        fixture.expression(wrapper_index, argument)
                    );
                }
                assert_eq!(p_end, p);
                assert_eq!(c_end, c);
                assert_eq!(
                    *p_point,
                    crate::ownership_checking::UnitDropPoint::CallReturn(*call)
                );
                assert_eq!(p_point, c_point);
                let indices = if child_first {
                    [10, 18, 27, 31]
                } else {
                    [7, 16, 27, 31]
                };
                let callee_loans = indices.map(|i| {
                    let Event::BorrowRegistered(loan) = &callee[i] else {
                        panic!("real callee loan")
                    };
                    loan.clone()
                });
                let wrapper_loans = [p.clone(), c.clone()];
                let expected_loans = if callee_first {
                    callee_loans
                        .iter()
                        .chain(wrapper_loans.iter())
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    wrapper_loans
                        .iter()
                        .chain(callee_loans.iter())
                        .cloned()
                        .collect::<Vec<_>>()
                };
                assert_eq!(owned.loans(), expected_loans);
                let drop_indices = if child_first {
                    [15, 23, 29, 33]
                } else {
                    [12, 21, 29, 33]
                };
                let drops = drop_indices.map(|i| {
                    let Event::Dropped(drop) = callee[i] else {
                        panic!("real callee drop")
                    };
                    drop
                });
                assert_eq!(owned.drops(), drops);
                let [end_c, end_p] = owned.direct_call_loan_ends() else {
                    panic!("two real ends")
                };
                for (end, loan, point, preceding) in
                    [(end_c, c_end, c_point, 0), (end_p, p_end, p_point, 1)]
                {
                    assert_eq!(end.loan(), loan);
                    assert_eq!(end.point(), *point);
                    assert_eq!(end.preceding_call_loan_ends(), preceding);
                    assert_eq!(end.preceding_drops(), 0);
                }
                let primitive_indices = if child_first { [12, 20] } else { [9, 18] };
                let calls = primitive_indices.map(|i| {
                    let Event::Applied { call, .. } = callee[i] else {
                        panic!("actual replacement")
                    };
                    call
                });
                assert_eq!(
                    owned
                        .ownership_primitives()
                        .iter()
                        .map(|p| p.descriptor().expression())
                        .collect::<Vec<_>>(),
                    calls
                );
                assert_eq!(owned.call_argument_contracts().len(), 8);
                assert_eq!(
                    fixture.names.names().index().source_units()[0].id(),
                    if callee_first {
                        fixture.source(callee_index)
                    } else {
                        fixture.source(wrapper_index)
                    }
                );
            }
        }
    }
}

fn role(event: &Event) -> &'static str {
    match event {
        Event::Literal(_) => "literal",
        Event::Bound(_, _) => "bound",
        Event::Applied { .. } => "replace",
        Event::Dropped(_) => "drop",
        Event::BorrowRegistered(_) => "borrow",
        Event::PendingBorrowRemoved { .. } => "removed",
        Event::UncoveredCall(_) => "call",
        Event::WhileTraversal {
            role: WhileRole::Enter,
            ..
        } => "enter",
        Event::WhileTraversal {
            role: WhileRole::ConditionEvaluated,
            ..
        } => "condition",
        Event::WhileTraversal {
            role: WhileRole::BodyEnter,
            ..
        } => "body",
        Event::WhileTraversal {
            role: WhileRole::BodyExit(Some(true)),
            ..
        } => "body_exit",
        Event::WhileTraversal {
            role: WhileRole::Exit(Some(true)),
            ..
        } => "exit",
        _ => panic!("unconsumed actual event {event:?}"),
    }
}

// Same Checker construction as the existing helper, with the inspected source explicit.
fn inspect_source(
    fixture: &Fixture,
    parts: &Parts,
    source_index: usize,
    inspect: impl FnOnce(&Checker<'_>),
) {
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
    let mut checker = Checker::new(
        &fixture.sources,
        &fixture.parsed[source_index],
        fixture.source(source_index),
        fixture.names.names(),
        fixture.typed.types(),
        &parts.bindings,
        &fields,
        &parts.arguments,
        &parts.receivers,
        &capture_index,
        &parts.capture.closures,
        &parts.capture.transferabilities,
        constructions
            .get(&fixture.source(source_index))
            .unwrap_or(&empty),
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
    checker.constant_control = false;
    let drop_analysis = checker.run().unwrap();
    assert!(drop_analysis.deferred.is_empty());
    assert!(checker.primitive_deferred.is_empty());
    inspect(&checker);
}
