//! The real held owner is observed after an incoming move on every actual If path.
use super::*;
const SOURCE: &str = r#"fun run(flag: Boolean, other: Boolean) { val seed = "outer"; val held = seed; while(flag) { val local = "inner"; if(other) {} else {} }; val retired = held }"#;
#[test]
fn real_named_input_reaches_the_actual_normal_join() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let joined = owned.owner_observations()[0]
        .invariant_normal_join_observation_v1()
        .expect("actual named input through both normal arms");
    assert!(joined.is_consistent());
    assert_eq!(joined.entry(), joined.join_boundary());
    assert_eq!(joined.entry(), joined.backedge());
    compare(&fixture, &fixture.parts(&owned));
}
