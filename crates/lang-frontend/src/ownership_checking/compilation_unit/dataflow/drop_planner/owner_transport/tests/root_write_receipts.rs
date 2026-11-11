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
        assert_eq!(writes[0].branch_arms.len(), 2);
        assert_eq!(writes[1].branch_arms.len(), 2);
        assert_eq!(writes[2].branch_arms.len(), 1);
        assert!(writes[3].branch_arms.is_empty());
        assert_eq!(writes[0].branch_arms[0], writes[1].branch_arms[0]);
        assert_eq!(writes[0].branch_arms[0].0, writes[2].branch_arms[0].0);
        assert_eq!(writes[0].branch_arms[0].1, 0);
        assert_eq!(writes[2].branch_arms[0].1, 1);
        assert_eq!(writes[0].branch_arms[1].0, writes[1].branch_arms[1].0);
        assert_eq!(writes[0].branch_arms[1].1, 0);
        assert_eq!(writes[1].branch_arms[1].1, 1);
        assert_eq!(visits[1].root_stages, (true, false));
    });
}

#[test]
fn actual_root_writes_keep_mixed_when_if_body_ancestry_and_restore_at_join() {
    let text = SOURCE.replace(
        "val first = replace(&left, \"next-left\")",
        "when { flag -> { if(flag) { val a = replace(&left, \"a\") } else { val b = replace(&left, \"b\") } }; else -> { val c = replace(&left, \"c\") } }",
    );
    let fixture = Fixture::new(&[("root-write-when-arms.ko", &text)]);
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
        assert_eq!(writes[0].branch_arms.len(), 2);
        assert_eq!(writes[1].branch_arms.len(), 2);
        assert_eq!(writes[2].branch_arms.len(), 1);
        assert!(writes[3].branch_arms.is_empty());
        assert_eq!(writes[0].branch_arms[0], writes[1].branch_arms[0]);
        assert_eq!(writes[0].branch_arms[0].0, writes[2].branch_arms[0].0);
        assert_eq!(writes[0].branch_arms[0].1, 0);
        assert_eq!(writes[2].branch_arms[0].1, 1);
        assert_eq!(writes[0].branch_arms[1].0, writes[1].branch_arms[1].0);
        assert_eq!(writes[0].branch_arms[1].1, 0);
        assert_eq!(writes[1].branch_arms[1].1, 1);
        assert_eq!(visits[1].root_stages, (true, false));
    });
}

#[test]
fn actual_root_writes_keep_string_when_body_identity_not_selector_history() {
    let text = SOURCE.replace(
        "val first = replace(&left, \"next-left\")",
        "when(\"choose\") { \"a\", \"b\" -> { if(flag) { val a = replace(&left, \"a\") } else { val b = replace(&left, \"b\") } }; else -> { val c = replace(&left, \"c\") } }",
    );
    let fixture = Fixture::new(&[("root-write-string-when-arms.ko", &text)]);
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
        assert_eq!(writes[0].branch_arms.len(), 2);
        assert_eq!(writes[1].branch_arms.len(), 2);
        assert_eq!(writes[2].branch_arms.len(), 1);
        assert!(writes[3].branch_arms.is_empty());
        assert_eq!(writes[0].branch_arms[0], writes[1].branch_arms[0]);
        assert_eq!(writes[0].branch_arms[0].0, writes[2].branch_arms[0].0);
        assert_eq!(writes[0].branch_arms[0].1, 0);
        assert_eq!(writes[2].branch_arms[0].1, 1);
        assert_eq!(writes[0].branch_arms[1].0, writes[1].branch_arms[1].0);
        assert_eq!(writes[0].branch_arms[1].1, 0);
        assert_eq!(writes[1].branch_arms[1].1, 1);
        assert_eq!(visits[1].root_stages, (true, false));
    });
}

#[test]
fn actual_root_writes_qualify_evaluated_short_circuit_rhs_and_restore() {
    for (left, operator, branch, expected) in [
        ("flag", "&&", 0, 2),
        ("flag", "||", 1, 2),
        ("false", "&&", 0, 1),
        ("true", "||", 1, 1),
        ("true", "&&", 0, 2),
        ("false", "||", 1, 2),
    ] {
        let replacement = format!(
            "if({left} {operator} when {{ flag -> {{ val old = replace(&left, \"rhs\"); true }}; else -> false }}) {{ }}"
        );
        let text = SOURCE.replace("val first = replace(&left, \"next-left\")", &replacement);
        let fixture = Fixture::new(&[("root-write-short-circuit.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.clone().validate().is_ok());
        for constant_control in [false, true] {
            compare_and_inspect_control_mode(
                &fixture,
                &fixture.parts(&owned),
                constant_control,
                |checker| {
                    let expected = if constant_control { expected } else { 2 };
                    let mut planner = super::super::super::DropPlanner::new(
                        checker,
                        super::super::super::liveness::build(checker, true).unwrap(),
                    );
                    planner.owner_control_probe = Some(Vec::new());
                    let planner = planner.run().unwrap();
                    let visits = planner.owner_control_probe.as_ref().unwrap();
                    let writes = &visits[1].root_writes;
                    assert_eq!(writes.len(), expected, "{text}");
                    assert!(writes.last().unwrap().branch_arms.is_empty());
                    if expected == 2 {
                        assert_eq!(writes[0].branch_arms.len(), 2);
                        assert_eq!(writes[0].branch_arms[0].1, branch);
                        assert_eq!(writes[0].branch_arms[1].1, 0);
                        assert!(
                            checker
                                .short_circuit_plan(writes[0].branch_arms[0].0.expression())
                                .unwrap()
                                .is_some()
                        );
                    }
                },
            );
        }
    }
}

#[test]
fn actual_two_root_normal_body_records_only_complete_linear_write_vector() {
    let normal = SOURCE.replace("        break\n", "");
    for (text, admitted) in [
        (normal.clone(), true),
        (
            normal
                .replace("&left", "&temporary")
                .replace("&right", "&left")
                .replace("&temporary", "&right"),
            true,
        ),
        (normal.replace("replace(&right,", "replace(&left,"), false),
        (
            normal.replace(
                "val first = replace(&left, \"next-left\")",
                "if(flag) { val first = replace(&left, \"next-left\") }",
            ),
            false,
        ),
        (
            normal.replace("val second =", "val extra = \"extra\"; val second ="),
            false,
        ),
        (SOURCE.to_owned(), false),
        (SOURCE.replace("break", "continue"), false),
        (SOURCE.replace("break", "return"), false),
    ] {
        let fixture = Fixture::new(&[("root-normal-vector.ko", &text)]);
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
            let vector = &visits[1].normal_write_vector;
            assert_eq!(vector.is_some(), admitted, "{text}");
            assert_eq!(visits[1].normal_equation.is_some(), admitted, "{text}");
            assert_eq!(visits[1].normal_retirements.is_some(), admitted, "{text}");
            if !admitted {
                assert!(
                    visits[1]
                        .root_writes
                        .iter()
                        .all(|write| write.token_link.is_none())
                );
            }
            if let Some(vector) = vector {
                let retirements = visits[1].normal_retirements.as_ref().unwrap();
                assert_eq!(retirements.len(), vector.len());
                for (root, binding, fact) in retirements {
                    assert!(vector.iter().any(|(known, _)| known == root));
                    assert_eq!(
                        fact.target(),
                        crate::ownership_checking::UnitDropTarget::Named(*binding)
                    );
                    assert!(matches!(
                        fact.point(),
                        crate::ownership_checking::UnitDropPoint::AfterStatement(_)
                    ));
                }
                let roots = visits[0].incoming_roots.as_ref().unwrap();
                let equation = visits[1].normal_equation.as_ref().unwrap();
                assert_eq!(equation.control, visits[0].control);
                assert_eq!(&equation.entry, roots);
                assert_eq!(equation.normal.len(), roots.len());
                for ((root, new), (entry_root, old)) in equation.normal.iter().zip(roots) {
                    assert_eq!(root, entry_root);
                    assert_ne!(new, old);
                    assert!(visits[1].root_writes.iter().any(|write| {
                        write.root == *root
                            && write
                                .token_link
                                .as_ref()
                                .is_some_and(|(_, token)| token == new)
                    }));
                }
                assert_eq!(vector.len(), 2);
                for ((root, producer), (entry_root, _)) in vector.iter().zip(roots) {
                    assert_eq!(root, entry_root);
                    assert!(
                        visits[1]
                            .root_writes
                            .iter()
                            .any(|receipt| receipt.root == *root && receipt.producer == *producer)
                    );
                }
                for write in &visits[1].root_writes {
                    let (old, new) = write.token_link.as_ref().unwrap();
                    let (binding, drop) = write.token_retirement.as_ref().unwrap();
                    assert!(retirements.iter().any(
                        |(root, expected_binding, expected_drop)| *root == write.root
                            && binding == expected_binding
                            && drop == expected_drop
                    ));
                    assert_eq!(
                        old,
                        &roots
                            .iter()
                            .find(|(root, _)| *root == write.root)
                            .unwrap()
                            .1
                    );
                    assert_ne!(old, new);
                    assert!(!std::sync::Arc::ptr_eq(&old.identity, &new.identity));
                }
                let new0 = &visits[1].root_writes[0].token_link.as_ref().unwrap().1;
                let new1 = &visits[1].root_writes[1].token_link.as_ref().unwrap().1;
                assert_ne!(new0, new1);
                assert!(std::sync::Arc::ptr_eq(&new0.identity, &new1.identity));
                // Changed roots remain ineligible for the old unchanged-root backedge proof.
                assert_eq!(visits[1].root_stages, (true, false));
            }
        });
    }
}

#[test]
fn actual_multi_root_cleanup_does_not_publish_generation_retirement() {
    let text = SOURCE.replace("        break\n", "");
    let fixture = Fixture::new(&[("root-generation-frontier.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.clone().validate().is_ok());
    assert!(!owned.owner_observations().is_empty());
    for observation in owned.owner_observations() {
        assert!(matches!(
            observation.status(),
            crate::ownership_checking::UnitOwnerObservationStatus::Unsupported { .. }
        ));
        assert!(observation.definitions().is_empty());
        assert!(observation.events().is_empty());
        assert!(observation.value_observation_v1().is_none());
        assert!(observation.loop_observation_v1().is_none());
        assert!(observation.closed_loop_batch_v1().is_none());
        assert!(observation.open_loop_observation_v1().is_none());
        assert!(observation.invariant_loop_observation_v1().is_none());
        assert!(observation.replace_break_observation_v1().is_none());
    }
    let mut analyses = Vec::new();
    for _ in 0..2 {
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let visits = planner.owner_control_probe.as_ref().unwrap();
            assert_eq!(visits[1].normal_write_vector.as_ref().unwrap().len(), 2);
            assert_eq!(visits[1].normal_retirements.as_ref().unwrap().len(), 2);
            assert_eq!(visits[1].root_stages, (true, false));
            analyses.push(
                visits[1]
                    .root_writes
                    .iter()
                    .map(|write| write.token_link.as_ref().unwrap().1.clone())
                    .collect::<Vec<_>>(),
            );
        });
    }
    assert_eq!(analyses[0], analyses[0].clone());
    assert!(
        analyses[0]
            .iter()
            .zip(&analyses[1])
            .all(|(left, right)| left != right)
    );
}
