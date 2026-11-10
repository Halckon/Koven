//! The invariant owner enters the loop through one real named move, not a new producer.
use super::*;
const SOURCE: &str = r#"fun run(flag: Boolean) { val seed = "outer"; val held = seed; while(flag) { val local = "inner" }; val retired = held }"#;
#[test]
fn actual_named_move_supplies_the_loop_input() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    super::invariant_while::check(&fixture, 0, &owned, None);
    let boundary = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let outer = boundary
        .post_loop_transport_v1()
        .expect("actual incoming move and final consumption");
    assert!(outer.is_consistent());
    assert_eq!(outer.prefix().len(), 7);
    assert_eq!(outer.producers().definitions().len(), 1);
    compare(&fixture, &fixture.parts(&owned));
}
