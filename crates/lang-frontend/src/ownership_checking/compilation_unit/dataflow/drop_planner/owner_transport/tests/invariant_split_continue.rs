//! Two actual Continue paths retain independent nonempty owner evidence.
use super::*;

#[test]
fn actual_split_continues_preserve_each_nonempty_arm_before_discard() {
    let text = "fun run(flag: Boolean, other: Boolean) { val held = \"outer\"; while (flag) { val local = \"body\"; if (other) { continue } else { continue } }; val retired = held }";
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    assert!(
        owned.owner_observations()[0]
            .invariant_split_continue_observation_v1()
            .expect("missing actual nonempty two-arm Continue witness")
            .is_consistent()
    );
}
