//! Bare Break and condition-false exits share one actual post-loop consumer.
use super::*;

const SOURCE: &str = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; break }; val retired = held }"#;

#[test]
fn actual_break_suffix_consumes_the_unchanged_input() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let legacy = &owned.owner_observations()[0];
    assert!(
        legacy.value_observation_v1().is_none()
            && legacy.definitions().is_empty()
            && legacy.events().is_empty()
    );
    assert!(legacy.invariant_loop_observation_v1().is_none());
    let boundary = legacy.invariant_break_observation_v1().unwrap();
    assert!(boundary.is_consistent() && boundary.break_boundary().is_some());
    let graph = boundary
        .post_loop_transport_v1()
        .expect("actual post-Break input consumption");
    assert!(graph.is_consistent());
    assert_eq!(graph.producers().definitions().len(), 1);
    assert_eq!(graph.prefix().len(), 5);
    compare(&fixture, &fixture.parts(&owned));
}
