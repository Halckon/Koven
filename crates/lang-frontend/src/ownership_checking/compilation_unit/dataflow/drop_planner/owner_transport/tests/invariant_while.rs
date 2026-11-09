//! Real nonempty owner boundaries; no synthesized input owner or repeated trace.
use super::*;

#[test]
fn actual_outer_owner_survives_three_while_boundaries() {
    let text = r#"fun run(flag: Boolean) { val held = "outer"; while (flag) { val local = "inner" }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
    assert!(legacy.value_observation_v1().is_none());
    assert!(legacy.loop_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_observation_v1().is_some(),
        "missing actual nonempty loop boundaries"
    );
}
