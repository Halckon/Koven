//! Real suffix traversal closes the invariant input without rewriting the old observer.
use super::*;

const SOURCE: &str = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = held }"#;

#[test]
fn actual_suffix_closes_the_input_owner_after_normal_while() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let legacy = &owned.owner_observations()[0];
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    assert!(legacy.value_observation_v1().is_none());
    let boundaries = legacy.invariant_loop_observation_v1().unwrap();
    assert!(boundaries.is_consistent());
    let outer = boundaries
        .post_loop_transport_v1()
        .expect("real post-loop input consumption");
    assert!(outer.is_consistent());
    assert_eq!(outer.producers().definitions().len(), 1);
    assert_eq!(outer.prefix().len(), 5);
    compare(&fixture, &fixture.parts(&owned));
}
