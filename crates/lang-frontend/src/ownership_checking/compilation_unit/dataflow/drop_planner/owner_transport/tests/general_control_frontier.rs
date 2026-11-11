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
