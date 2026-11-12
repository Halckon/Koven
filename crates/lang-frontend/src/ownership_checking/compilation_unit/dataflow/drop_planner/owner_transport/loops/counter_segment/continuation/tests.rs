use super::super::super::super::tests::compare_and_inspect;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

#[test]
fn second_bridge_collects_actual_aliases_from_frozen_formal_exit() {
    for mode in 0..5 {
        let transfer = match mode {
            0 => "var a2 = a1; var b2 = b1;",
            1 => "val saved_b = b1; val saved_a = a1; var b2 = saved_b; var a2 = saved_a;",
            2 => "val extra = \"unused\"; var a2 = a1; var b2 = b1;",
            3 => "var a2 = a1;",
            _ => "val retired_b = b1; var a2 = a1; var b2 = \"new\";",
        };
        let second_root = if mode == 3 { "b1" } else { "b2" };
        let source = format!(
            r#"fun run(n: Int) {{
            var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
            while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) {{ val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }};
            var n2 = n; var g2 = n2 > 0; {transfer}
            while(g2) {{ val x = replace(&a2, "x2"); val y = replace(&{second_root}, "y2"); n2 = n2 - 1; g2 = n2 > 0 }}
        }}"#
        );
        let fixture = Fixture::new(&[("second-bridge.ko", &source)]);
        let owned = fixture.shared();
        assert!(
            owned.diagnostics().is_empty(),
            "mode {mode}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.control_bridge_evidence_v1().is_none()
                    && o.counter_batch_evidence_v1().is_none())
        );
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                crate::ownership_checking::compilation_unit::dataflow::liveness::build(
                    checker, true,
                )
                .unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let entries: Vec<_> = planner
                .owner_control_probe
                .as_ref()
                .unwrap()
                .iter()
                .filter(|v| v.entering)
                .collect();
            assert_eq!(entries.len(), 3);
            assert!(
                entries[1].segment_bridge.is_some(),
                "first bridge preserved, mode {mode}"
            );
            assert!(entries[2].segment_catalog.is_none());
            assert!(entries[2].private_counter.is_none());
            let continuation = entries[2].segment_continuation.as_ref();
            assert_eq!(continuation.is_some(), mode < 2, "mode {mode}");
            if let Some(c) = continuation {
                assert!(c.complete());
                assert_eq!(c.normal.statement(), entries[1].control);
                assert_ne!(c.normal.statement(), entries[2].control);
                assert_eq!(c.live.len(), 2);
                assert!(c.drops.is_empty());
                let pairs = c.pairs();
                assert_eq!(pairs.len(), 2);
                let roots = c.normal.header_roots();
                assert_eq!(pairs[0].0, roots[usize::from(mode == 1)]);
                assert_eq!(pairs[1].0, roots[usize::from(mode != 1)]);
                assert!(pairs.iter().all(|(_, target)| !roots.contains(target)));
                assert_ne!(pairs[0].1, pairs[1].1);
                assert!(entries[2].incoming_roots.is_none());
            }
        });
    }
}

#[test]
fn third_control_body_uses_actual_formal_events_without_publication() {
    for (reverse, unsupported) in [(false, false), (true, false), (false, true)] {
        let writes = if reverse {
            "val y = replace(&b2, \"y2\"); val x = replace(&a2, \"x2\");"
        } else {
            "val x = replace(&a2, \"x2\"); val y = replace(&b2, \"y2\");"
        };
        let extra = if unsupported {
            "val unrelated = \"unsupported\";"
        } else {
            ""
        };
        let source = format!(
            r#"fun run(n: Int) {{
            var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
            while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) {{ val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }};
            var n2 = n; var g2 = n2 > 0;
            val saved_b = b1; val saved_a = a1; var b2 = saved_b; var a2 = saved_a;
            while(g2) {{ {writes} {extra} n2 = n2 - 1; g2 = n2 > 0 }}
        }}"#
        );
        let fixture = Fixture::new(&[("third-body.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.control_bridge_evidence_v1().is_none()
                    && o.counter_batch_evidence_v1().is_none())
        );
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                crate::ownership_checking::compilation_unit::dataflow::liveness::build(
                    checker, true,
                )
                .unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let visits = planner.owner_control_probe.as_ref().unwrap();
            let third = visits.iter().filter(|v| v.entering).nth(2).unwrap();
            let continuation = third.segment_continuation.as_ref().unwrap();
            let (targets, callable) = continuation.body_inputs().unwrap();
            let alias_order = continuation.pairs();
            assert_eq!(targets, vec![alias_order[1].1, alias_order[0].1]);
            let finished = visits
                .iter()
                .find(|v| !v.entering && v.control == third.control)
                .unwrap();
            assert_eq!(finished.continuation_body.is_some(), !unsupported);
            assert!(finished.private_counter.is_none());
            assert!(finished.normal_equation.is_none());
            if let Some(proof) = &finished.continuation_body {
                assert!(proof.normal.is_consistent());
                assert_eq!(proof.normal.statement(), third.control);
                assert_eq!(proof.normal.header_roots(), targets);
                assert_eq!(proof.normal.body_catalog().callable(), callable);
                assert_eq!(proof.normal.steps().len(), 2);
                assert!(proof.tail.is_some());
                let (live, drops) = proof.exit.as_ref().unwrap();
                assert!(live.is_empty());
                assert_eq!(drops.len(), 2);
                for drop in drops {
                    assert_eq!(drop.point(), UnitDropPoint::LoopExit(third.control));
                    assert!(
                        matches!(drop.target(), UnitDropTarget::Named(root) if targets.contains(&root))
                    );
                }
                let prior_outputs = continuation.normal.normal_outputs();
                assert!(
                    proof
                        .normal
                        .normal_outputs()
                        .iter()
                        .all(|(_, token)| prior_outputs.iter().all(|(_, prior)| prior != token))
                );
            }
        });
    }
}
