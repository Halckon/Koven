//! Evidence from real condition and body-completion hooks, not fabricated header inputs.
use super::*;
const SOURCE: &str = r#"fun run(flag: Boolean) {
    var left = "left"
    var right = "right"
    while(flag) { val local = "local" }
    val finalLeft = left
    val finalRight = right
}"#;
#[test]
fn actual_multi_root_stages_capture_unchanged_condition_and_normal_backedge() {
    let fixture = Fixture::new(&[("root-stages.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.clone().validate().is_ok());
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.owner_control_probe = Some(Vec::new());
        let planner = planner.run().unwrap();
        let visits = planner.owner_control_probe.as_ref().unwrap();
        assert_eq!(visits[0].incoming_roots.as_ref().unwrap().len(), 2);
        assert_eq!(visits[1].root_stages, (true, true));
    });
    assert!(
        owned
            .owner_observations()
            .iter()
            .all(|o| o.loop_observation_v1().is_none()
                && o.open_loop_observation_v1().is_none()
                && o.closed_loop_batch_v1().is_none())
    );
}
