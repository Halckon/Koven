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

#[test]
fn actual_root_write_receipts_reject_non_direct_rhs_and_unreached_commit() {
    for (text, expected) in [
        (
            SOURCE.replace(
                "replace(&left, \"next-left\")",
                "replace(&left, (\"next-left\"))",
            ),
            1,
        ),
        (
            SOURCE.replace(
                "val first = replace(&left, \"next-left\")",
                "val incoming = \"named\"; val first = replace(&left, incoming)",
            ),
            1,
        ),
        (
            SOURCE.replace("replace(&left, \"next-left\")", "replace(&left, return)"),
            0,
        ),
    ] {
        let fixture = Fixture::new(&[("root-write-frontier.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.clone().validate().is_ok());
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let visits = planner.owner_control_probe.as_ref().unwrap();
            assert_eq!(visits[1].root_writes.len(), expected, "{text}");
            if expected == 1 {
                assert_eq!(
                    visits[1].root_writes[0].root,
                    visits[0].incoming_roots.as_ref().unwrap()[1].0
                );
            }
        });
    }
}

#[test]
fn actual_repeated_root_writes_keep_distinct_producer_commits_in_order() {
    let text = SOURCE.replace(
        "replace(&right, \"next-right\")",
        "replace(&left, \"again\")",
    );
    let fixture = Fixture::new(&[("same-root-writes.ko", &text)]);
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
        let writes = &visits[1].root_writes;
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].root, writes[1].root);
        assert_ne!(writes[0].producer, writes[1].producer);
        let ast = checker.parsed.ast();
        assert!(
            ast.expressions()
                .get(writes[0].call.expression())
                .unwrap()
                .span()
                .start()
                < ast
                    .expressions()
                    .get(writes[1].call.expression())
                    .unwrap()
                    .span()
                    .start()
        );
    });
}

#[test]
fn actual_root_writes_keep_nested_if_arm_ancestry_and_restore_at_join() {
    let text = SOURCE.replace(
        "val first = replace(&left, \"next-left\")",
        "if(flag) { if(flag) { val a = replace(&left, \"a\") } else { val b = replace(&left, \"b\") } } else { val c = replace(&left, \"c\") }",
    );
    let fixture = Fixture::new(&[("root-write-if-arms.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.clone().validate().is_ok());
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.owner_control_probe = Some(Vec::new());
        let planner = planner.run().unwrap();
        let visits = planner.owner_control_probe.as_ref().unwrap();
        let writes = &visits[1].root_writes;
        assert_eq!(writes.len(), 4);
        assert_eq!(writes[0].if_arms.len(), 2);
        assert_eq!(writes[1].if_arms.len(), 2);
        assert_eq!(writes[2].if_arms.len(), 1);
        assert!(writes[3].if_arms.is_empty());
        assert_eq!(writes[0].if_arms[0], writes[1].if_arms[0]);
        assert_eq!(writes[0].if_arms[0].0, writes[2].if_arms[0].0);
        assert_eq!(writes[0].if_arms[0].1, 0);
        assert_eq!(writes[2].if_arms[0].1, 1);
        assert_eq!(writes[0].if_arms[1].0, writes[1].if_arms[1].0);
        assert_eq!(writes[0].if_arms[1].1, 0);
        assert_eq!(writes[1].if_arms[1].1, 1);
        assert_eq!(visits[1].root_stages, (true, false));
    });
}
