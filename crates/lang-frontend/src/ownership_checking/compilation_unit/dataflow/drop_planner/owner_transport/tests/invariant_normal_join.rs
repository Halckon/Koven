//! Nonempty normal arms must retain a real post-merge state.
use super::*;

#[test]
fn actual_normal_arms_preserve_owner_through_the_real_join() {
    let text = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) {} else {} }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.invariant_loop_observation_v1().is_none());
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    assert!(legacy.invariant_split_exit_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_value.is_some(),
        "missing actual nonempty normal If join witness"
    );
}
