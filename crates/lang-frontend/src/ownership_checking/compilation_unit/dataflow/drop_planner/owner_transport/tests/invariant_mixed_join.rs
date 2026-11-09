//! A real normal continuation must stay separate from its terminated sibling.
use super::*;

#[test]
fn actual_mixed_arms_preserve_the_surviving_join_and_jump() {
    let text = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) {} else { continue } }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.invariant_normal_join_observation_v1().is_none());
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    assert!(legacy.invariant_split_exit_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_value.is_some(),
        "missing actual mixed normal/jump witness"
    );
}
