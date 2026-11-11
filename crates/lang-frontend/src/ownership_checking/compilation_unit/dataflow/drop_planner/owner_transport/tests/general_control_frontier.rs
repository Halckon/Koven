//! Real legal general-control inputs retained before replacing the single transaction.
//! These assert the current capability boundary, not generalized transport completion.
use super::*;
use crate::ownership_checking::{UnitOwnerCallable, UnitOwnerObservationStatus};
use crate::type_checking::UnitItemId;

const TWO_ROOTS: &str = r#"fun run(limit: Int) {
    var left = "left"
    var right = "right"
    var index = 0
    while(index < limit) {
        val oldLeft = replace(&left, "next-left")
        val oldRight = replace(&right, "next-right")
        index = index + 1
    }
    val finalLeft = left
    val finalRight = right
}"#;
const NESTED: &str = r#"fun run(limit: Int) {
    var outer = "outer"
    var index = 0
    while(index < limit) {
        var inner = "inner"
        var nested = 0
        while(nested < limit) {
            val oldInner = replace(&inner, "next-inner")
            nested = nested + 1
        }
        val finalInner = inner
        val oldOuter = replace(&outer, "next-outer")
        index = index + 1
    }
    val finalOuter = outer
}"#;

fn assert_frontier(text: &str) {
    let fixture = Fixture::new(&[("frontier.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(owned.clone().validate().is_ok());
    let callable = UnitOwnerCallable::Function(UnitItemId::new(
        fixture.source(0),
        fixture.parsed[0].roots()[0],
    ));
    let observation = owned
        .owner_observations()
        .iter()
        .find(|o| o.callable() == callable)
        .unwrap();
    assert!(matches!(
        observation.status(),
        UnitOwnerObservationStatus::Unsupported { .. }
    ));
    assert!(observation.definitions().is_empty());
    assert!(observation.events().is_empty());
    assert!(observation.open_loop_observation_v1().is_none());
    assert!(observation.loop_observation_v1().is_none());
    assert!(observation.invariant_loop_observation_v1().is_none());
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn general_control_frontier_two_roots_preserve_legacy_without_single_root_promotion() {
    assert_frontier(TWO_ROOTS);
    assert_frontier(&TWO_ROOTS.replace("index = index + 1", "index = index + 1; continue"));
}
#[test]
fn general_control_frontier_nested_loops_preserve_legacy_without_flattening() {
    assert_frontier(NESTED);
    assert_frontier(&NESTED.replace("nested = nested + 1", "nested = nested + 1; continue"));
    assert_frontier(&NESTED.replace("nested = nested + 1", "nested = nested + 1; break"));
}

#[test]
fn nested_control_stack_records_actual_parent_and_restores_after_jump() {
    for text in [
        NESTED.to_owned(),
        NESTED.replace("nested = nested + 1", "nested = nested + 1; continue"),
        NESTED.replace("nested = nested + 1", "nested = nested + 1; break"),
        NESTED.replace("nested = nested + 1", "nested = nested + 1; return"),
    ] {
        let fixture = Fixture::new(&[("scopes.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let visits = planner.owner_control_probe.as_ref().unwrap();
            assert_eq!(visits.len(), 4);
            assert!(visits[0].entering);
            assert_eq!(visits[0].parent, None);
            assert!(visits[1].entering);
            assert_ne!(visits[0].control, visits[1].control);
            assert_eq!(visits[1].parent, Some(visits[0].control));
            assert!(!visits[2].entering);
            assert_eq!(visits[2].control, visits[1].control);
            assert_eq!(visits[2].parent, Some(visits[0].control));
            assert!(!visits[3].entering);
            assert_eq!(visits[3].control, visits[0].control);
            assert_eq!(visits[3].parent, None);
        });
    }
}

#[test]
fn sequential_control_stack_does_not_invent_parent_or_retain_previous_scope() {
    let fixture = Fixture::new(&[(
        "sequential.ko",
        r#"fun run(flag: Boolean) {
        while(flag) { val first = "first"; break }
        while(flag) { val second = "second"; break }
    }"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.clone().validate().is_ok());
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.owner_control_probe = Some(Vec::new());
        let planner = planner.run().unwrap();
        let visits = planner.owner_control_probe.as_ref().unwrap();
        assert_eq!(visits.len(), 4);
        assert!(visits.iter().all(|v| v.parent.is_none()));
        assert_eq!(visits[0].control, visits[1].control);
        assert_eq!(visits[2].control, visits[3].control);
        assert_ne!(visits[0].control, visits[2].control);
        assert_eq!(
            visits.iter().map(|v| v.entering).collect::<Vec<_>>(),
            [true, false, true, false]
        );
        assert!(planner.owner_controls.balanced());
    });
}

#[test]
fn ci_only_control_stack_invalid_entry_and_wrong_exit_stay_invalid() {
    let fixture = Fixture::new(&[("invalid-stack.ko", NESTED)]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.owner_control_probe = Some(Vec::new());
        let planner = planner.run().unwrap();
        let visits = planner.owner_control_probe.as_ref().unwrap();
        let outer = visits[0].control;
        let inner = visits[1].control;
        let mut duplicate = super::super::loops::ControlStack::default();
        assert!(duplicate.enter(outer));
        assert!(!duplicate.enter(outer));
        duplicate.leave(outer);
        assert!(!duplicate.balanced());
        assert!(!duplicate.enter(inner));
        let mut mismatch = super::super::loops::ControlStack::default();
        assert!(mismatch.enter(outer));
        assert!(mismatch.enter(inner));
        mismatch.leave(outer);
        assert!(!mismatch.is_current(inner));
        mismatch.leave(inner);
        mismatch.leave(outer);
        assert!(!mismatch.balanced());
    });
}

#[test]
fn control_transaction_registry_isolates_real_callable_controls_and_restores_prefix_slot() {
    let text = r#"fun first(flag: Boolean): String {
        var held = "outer"
        var again = flag
        while(again) { val previous = replace(&held, "next"); again = false }
        return held
    }
    fun second(flag: Boolean): String {
        var held = "other"
        var again = flag
        while(again) { val previous = replace(&held, "second"); again = false }
        return held
    }"#;
    let fixture = Fixture::new(&[("transaction-controls.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.clone().validate().is_ok());
    use crate::ownership_checking::UnitOwnerOpenLoopObservationV1;
    let products: Vec<_> = owned
        .owner_observations()
        .iter()
        .filter_map(|o| match o.open_loop_observation_v1() {
            Some(UnitOwnerOpenLoopObservationV1::NormalBackedge(product)) => Some(product),
            _ => None,
        })
        .collect();
    assert_eq!(
        products.len(),
        2,
        "both real admitted functions publish their old product"
    );
    assert_ne!(products[0].callable(), products[1].callable());
    assert_ne!(products[0].statement(), products[1].statement());
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        use super::super::loops::ControlTransactions;
        let roots = checker.parsed.roots();
        let identity = std::sync::Arc::new(());
        let mut first = ControlTransactions::begin(checker, roots[0], identity.clone());
        let second = ControlTransactions::begin(checker, roots[1], identity);
        let original = first.selected().expect("first actual admitted control");
        let other = second.selected().expect("second actual admitted control");
        assert_ne!(original, other);
        let transaction = first.as_ref().unwrap() as *const _;
        first.select(Some(other));
        assert!(first.as_ref().is_none());
        assert!(first.as_mut().is_none());
        assert!(
            first.is_some(),
            "inactive entries still block incompatible borrow admission"
        );
        first.select(None);
        assert!(first.as_ref().is_none());
        first.select(Some(original));
        assert_eq!(first.as_ref().unwrap() as *const _, transaction);
        assert!(first.take().is_some());
        assert!(first.is_none());
        assert!(first.selected().is_none());
    });
}
