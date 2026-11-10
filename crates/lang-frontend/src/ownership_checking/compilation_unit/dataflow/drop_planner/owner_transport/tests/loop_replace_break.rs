//! Real admitted source, not a manufactured owner model.
use super::*;
const SOURCE: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }"#;
#[test]
fn actual_replace_break_preserves_distinct_exit_owners() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observed = owned.owner_observations()[0]
        .replace_break_observation_v1()
        .expect("actual Replace then Break must publish separate P/Q exits");
    assert!(observed.is_consistent());
}
