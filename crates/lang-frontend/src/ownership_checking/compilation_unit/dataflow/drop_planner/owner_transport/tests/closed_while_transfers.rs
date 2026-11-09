//! Real terminal jumps in a closed While body, not a false return inferred as a backedge.
use super::*;

#[test]
fn real_while_terminal_jumps_publish_closed_transport() {
    for jump in ["continue", "break"] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ while (flag) {{ val local = "local"; {jump} }} }}"#
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{text}");
        assert!(
            owned.owner_observations()[0]
                .loop_observation_v1()
                .is_some(),
            "missing actual terminal jump: {text}"
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}
