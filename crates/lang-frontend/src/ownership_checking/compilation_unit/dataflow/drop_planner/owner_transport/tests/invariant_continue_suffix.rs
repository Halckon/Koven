//! A real Continue backedge does not replace the condition-false suffix consumer.
use super::*;

const SOURCE: &str = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; continue }; val retired = held }"#;

#[test]
fn actual_continue_suffix_consumes_only_after_condition_false_exit() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let jump = fixture.expression(0, "continue");
    super::invariant_while::check(&fixture, 0, &owned, Some(jump));
    let boundaries = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    assert_eq!(
        boundaries.continue_boundary(),
        Some((
            UnitExpressionId::new(fixture.source(0), jump),
            boundaries.entry()
        ))
    );
    let graph = boundaries
        .post_loop_transport_v1()
        .expect("actual suffix consumes the unchanged input after condition-false exit");
    assert!(graph.is_consistent());
    assert_eq!(graph.prefix().len(), 5);
    assert_eq!(graph.producers().definitions().len(), 1);
    compare(&fixture, &fixture.parts(&owned));
}
