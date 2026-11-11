//! Actual traversal completion, not a hand-populated transaction registry.
use super::*;

#[test]
fn sequential_closed_transactions_capture_each_real_control_without_single_publication() {
    for tail in ["", "; continue", "; break"] {
        let text = format!(
            r#"fun run(flag: Boolean) {{
            while(flag) {{ val first = "first"{tail} }}
            while(flag) {{ val second = "second"; break }}
        }}"#
        );
        let fixture = Fixture::new(&[("sequential-closed.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.clone().validate().is_ok());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.loop_observation_v1().is_none())
        );
        assert!(owned.owner_observations().iter().all(|o| matches!(
            o.status(),
            crate::ownership_checking::UnitOwnerObservationStatus::Unsupported { .. }
        )));
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let visits = planner.owner_control_probe.as_ref().unwrap();
            let exits: Vec<_> = visits.iter().filter(|visit| !visit.entering).collect();
            assert_eq!(exits.len(), 2);
            let first = exits[0]
                .closed_product
                .as_ref()
                .expect("first real closed completion");
            let second = exits[1]
                .closed_product
                .as_ref()
                .expect("second real closed completion");
            assert!(first.is_consistent());
            assert!(second.is_consistent());
            assert_eq!(first.statement(), exits[0].control);
            assert_eq!(second.statement(), exits[1].control);
            assert_ne!(first.statement(), second.statement());
            assert_eq!(first.callable, second.callable);
            assert!(std::sync::Arc::ptr_eq(&first.identity, &second.identity));
            assert!(!std::sync::Arc::ptr_eq(
                &first.body_identity,
                &second.body_identity
            ));
            assert!(planner.owner_controls.balanced());
        });
    }
}

#[test]
fn sequential_closed_transactions_decline_entire_batch_when_any_member_is_outside_contract() {
    for remainder in [
        r#"while(flag) { val second = "second"; return }"#,
        r#"while(flag) { val second = "second"; while(flag) { val nested = "nested"; break }; break }"#,
        r#"while(flag) { val second = "second"; break }; val outside = "outside""#,
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ while(flag) {{ val first = "first"; break }}; {remainder} }}"#
        );
        let fixture = Fixture::new(&[("sequence-boundary.ko", &text)]);
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
            assert!(
                planner
                    .owner_control_probe
                    .as_ref()
                    .unwrap()
                    .iter()
                    .all(|v| v.closed_product.is_none()),
                "reject admission atomically, do not retain the first valid member"
            );
        });
    }
}

#[test]
fn ci_only_unbalanced_transaction_finalization_does_not_publish_empty_complete_observation() {
    let fixture = Fixture::new(&[(
        "unbalanced-finalization.ko",
        r#"fun run(flag: Boolean) { while(flag) { val local = "local"; break } }"#,
    )]);
    let owned = fixture.shared();
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.begin_owner_function(checker.parsed.roots()[0]);
        let control = planner.owner_loop.selected().unwrap();
        assert!(planner.owner_controls.enter(control));
        planner.finish_owner_observation();
        let observation = planner.owner_observations.last().unwrap();
        assert!(matches!(
            observation.status(),
            crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
                reason:
                    crate::ownership_checking::UnitOwnerUnsupportedReason::InconsistentTransport,
                ..
            }
        ));
        assert!(observation.loop_observation_v1().is_none());
        assert!(observation.value_observation_v1().is_none());
    });
}
