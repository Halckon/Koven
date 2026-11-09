//! Actual conditional jump traversal after a closed owner prefix.
use super::*;

#[test]
fn real_while_tail_if_records_both_completed_jump_edges() {
    let text = r#"fun run(flag: Boolean, exit: Boolean) {
        while (flag) { val local = "local"; if (exit) { break } else { continue } }
    }"#;
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert!(
        owned.owner_observations()[0]
            .loop_observation_v1()
            .is_some(),
        "missing actual terminal If edges"
    );
    compare(&fixture, &fixture.parts(&owned));
}
