//! A real Continue must independently witness its surviving input owner.
use super::*;

#[test]
fn actual_continue_retains_the_nonempty_input_owner() {
    // Preserve the exact former §152 legal Continue rejection as a new behavior witness.
    let text = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; continue }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
    assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_observation_v1().is_some(),
        "missing real nonempty Continue boundary"
    );
}
