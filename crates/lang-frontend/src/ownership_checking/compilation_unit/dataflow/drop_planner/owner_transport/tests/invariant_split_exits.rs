//! Real conditional Break destinations retain their own nonempty owner evidence.
use super::*;

#[test]
fn actual_split_exits_preserve_nonempty_owners_at_each_real_destination() {
    let text = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) { continue } else { break } }; val retired = held }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    assert!(
        legacy
            .invariant_split_exit_observation_v1()
            .expect("missing actual nonempty conditional Break exit witness")
            .is_consistent()
    );
}
