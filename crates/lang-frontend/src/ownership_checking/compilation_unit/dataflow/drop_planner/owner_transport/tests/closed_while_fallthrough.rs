//! Real empty-arm completion and the separate enclosing While body backedge.
use super::*;

#[test]
fn real_empty_arm_completion_retains_a_separate_body_backedge() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean, exit: Boolean) { while(flag) { val local = "local"; if(exit) { break } else {} } }"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let [observation] = owned.owner_observations() else {
        panic!("one actual callable");
    };
    let observed = observation
        .loop_observation_v1()
        .expect("actual empty arm completion");
    assert_eq!(observed.edges().len(), 7);
    assert!(observed.is_consistent());
}
