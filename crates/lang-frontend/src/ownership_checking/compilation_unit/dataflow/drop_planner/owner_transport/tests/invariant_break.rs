//! Real Break cleanup must not be passed off as a loop backedge.
use super::*;

#[test]
fn actual_break_preserves_the_owner_through_distinct_exit_states() {
    let text = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; break }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
    assert!(legacy.value_observation_v1().is_none());
    assert!(
        legacy.loop_observation_v1().is_none() && legacy.invariant_loop_observation_v1().is_none()
    );
    assert!(
        legacy.invariant_break_observation_v1().is_some(),
        "missing actual nonempty Break exit witness"
    );
}
