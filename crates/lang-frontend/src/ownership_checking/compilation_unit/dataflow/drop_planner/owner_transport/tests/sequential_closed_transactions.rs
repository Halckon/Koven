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
            assert!(planner.owner_controls.balanced());
        });
    }
}
