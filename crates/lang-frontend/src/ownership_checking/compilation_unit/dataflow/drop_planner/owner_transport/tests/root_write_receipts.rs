//! Producer completion and actual primitive commit must both be observed.
use super::*;
const SOURCE: &str = r#"fun run(flag: Boolean) {
    var left = "left"
    var right = "right"
    while(flag) {
        val first = replace(&left, "next-left")
        val second = replace(&right, "next-right")
        break
    }
    val finalLeft = left
    val finalRight = right
}"#;
#[test]
fn actual_two_root_writes_link_each_committed_direct_string_producer() {
    let fixture = Fixture::new(&[("root-writes.ko", SOURCE)]);
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
        let incoming = visits[0].incoming_roots.as_ref().unwrap();
        let writes = &visits[1].root_writes;
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].root, incoming[0].0);
        assert_eq!(writes[1].root, incoming[1].0);
        assert!(
            writes
                .iter()
                .all(|write| write.control == visits[0].control)
        );
        assert_ne!(writes[0].call, writes[1].call);
        assert_ne!(writes[0].producer, writes[1].producer);
        assert_eq!(visits[1].root_stages, (true, false));
    });
}
