//! Remote-CI-only failure injection; normal local tests leave the switch disabled.
use super::super::super::super::tests::compare_and_inspect;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
thread_local! { static FAILURE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
pub(super) fn injected_failure(phase: u8) -> bool {
    FAILURE.get() == phase
}
struct Restore(u8);
impl Drop for Restore {
    fn drop(&mut self) {
        FAILURE.set(self.0);
    }
}

#[test]
fn ci_only_token_evidence_requires_entry_and_complete_events() {
    let fixture = Fixture::new(&[(
        "token-revoke.ko",
        "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val keptA = a; val keptB = b }",
    )]);
    let owned = fixture.shared();
    for phase in [0, 1, 2, 3, 4, 5] {
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                super::super::super::super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let restore = Restore(FAILURE.replace(phase));
            let planner = planner.run().unwrap();
            drop(restore);
            let visits = planner.owner_control_probe.as_ref().unwrap();
            let writes = &visits[1].root_writes;
            assert_eq!(writes.len(), 2);
            assert!(visits[1].normal_write_vector.is_some());
            assert!(visits[1].normal_retirements.is_some());
            assert_eq!(visits[1].normal_equation.is_some(), phase == 0);
            if phase == 0 {
                let equation = visits[1].normal_equation.as_ref().unwrap();
                let mut mixed = equation.entry.clone();
                mixed[0] = equation.normal[0].clone();
                assert!(equation.predecessor_for(equation.control, &mixed).is_none());
                let mut swapped = equation.normal.clone();
                swapped.swap(0, 1);
                assert!(
                    equation
                        .predecessor_for(equation.control, &swapped)
                        .is_none()
                );
                let mut missing = equation.normal.clone();
                missing.pop();
                assert!(
                    equation
                        .predecessor_for(equation.control, &missing)
                        .is_none()
                );
                assert!(writes.iter().all(|write| write.token_link.is_some() && write.token_retirement.is_some()));
            } else {
                assert!(writes.iter().all(|write| write.token_link.is_none() && write.token_retirement.is_none()));
            }
        });
    }
}

#[test]
fn counter_tail_records_actual_order_but_withholds_public_boundary() {
    for (tail, accepted) in [
        ("remaining = remaining - 1; again = remaining > 0", true),
        ("remaining = remaining - 2; again = remaining > 0", false),
        ("remaining = other - 1; again = remaining > 0", false),
        ("remaining = remaining - 1; again = other > 0", false),
        ("remaining = remaining - 1; again = remaining >= 0", false),
        ("again = remaining > 0; remaining = remaining - 1", false),
        (
            "remaining = remaining - 1; again = remaining > 0; val extra = 1",
            false,
        ),
    ] {
        for (initial, supported) in [
            ("flag", true),
            ("remaining > 0", true),
            ("other > 0", false),
            ("remaining >= 0", false),
            ("remaining + 1 > 0", false),
        ] {
            let text = format!(
                "fun run(n: Int, flag: Boolean) {{ var remaining = n; val other = n; var again = {initial}; var a = \"a\"; var b = \"b\"; while(again) {{ val x = replace(&a, \"x\"); val y = replace(&b, \"y\"); {tail} }} }}"
            );
            let fixture = Fixture::new(&[("counter-tail-contract.ko", &text)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty());
            assert!(
                owned
                    .owner_observations()
                    .iter()
                    .all(|o| o.multi_root_boundary_v1().is_none())
            );
            assert!(
                owned.owner_observations().iter().all(|o| o.status()
                    != crate::ownership_checking::UnitOwnerObservationStatus::Complete)
            );
            compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
                let mut planner = DropPlanner::new(
                    checker,
                    super::super::super::super::super::super::liveness::build(checker, true)
                        .unwrap(),
                );
                planner.owner_control_probe = Some(Vec::new());
                let planner = planner.run().unwrap();
                let equation = planner
                    .owner_control_probe
                    .as_ref()
                    .unwrap()
                    .iter()
                    .find_map(|visit| visit.normal_equation.as_ref());
                assert_eq!(
                    equation.is_some(),
                    accepted && supported,
                    "{tail}: {:?}",
                    planner.owner_control_probe
                );
                if let Some(equation) = equation {
                    assert!(equation.counter.as_ref().unwrap().is_complete());
                    assert!(equation.clone().into_boundary().is_none());
                }
            });
        }
    }
}

#[test]
fn counter_prefix_rejects_intervening_computation_or_call() {
    for (extra, accepted) in [
        ("val harmless = n;", true),
        ("val changed = n + 1;", false),
        ("val called = identity(n);", false),
    ] {
        let text = format!(
            "fun identity(n: Int): Int {{ return n }}\nfun run(n: Int) {{ var remaining = n; var again = remaining > 0; {extra} var a = \"a\"; var b = \"b\"; while(again) {{ val x = replace(&a, \"x\"); val y = replace(&b, \"y\"); remaining = remaining - 1; again = remaining > 0 }} }}"
        );
        let fixture = Fixture::new(&[("counter-prefix-effects.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.multi_root_boundary_v1().is_none())
        );
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                super::super::super::super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            assert_eq!(
                planner
                    .owner_control_probe
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|visit| visit.normal_equation.is_some()),
                accepted,
                "{extra}"
            );
        });
    }
}

#[test]
fn two_counter_candidates_do_not_publish_a_partial_first_product() {
    let segment = |i| {
        format!(
            "var n{i} = n; var g{i} = n{i} > 0; var a{i} = \"a\"; var b{i} = \"b\"; while(g{i}) {{ val x{i} = replace(&a{i}, \"x\"); val y{i} = replace(&b{i}, \"y\"); n{i} = n{i} - 1; g{i} = n{i} > 0; }};"
        )
    };
    let text = format!("fun run(n: Int) {{ {} {} }}", segment(0), segment(1));
    let fixture = Fixture::new(&[("two-counter-controls.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(
        owned
            .owner_observations()
            .iter()
            .all(|o| o.counter_tail_evidence_v1().is_none())
    );
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = DropPlanner::new(
            checker,
            super::super::super::super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.owner_control_probe = Some(Vec::new());
        let planner = planner.run().unwrap();
        let visits = planner.owner_control_probe.as_ref().unwrap();
        let entries: Vec<_> = visits
            .iter()
            .filter(|v| v.entering && v.counter_phase.is_some())
            .collect();
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].control, entries[1].control);
        assert!(entries[0].incoming_roots.is_some());
        assert!(entries[1].incoming_roots.is_none());
        assert_eq!(
            entries[0].counter_phase.unwrap(),
            super::super::super::control_transactions::CounterPhase::Entered
        );
        assert_eq!(
            entries[1].counter_phase.unwrap(),
            super::super::super::control_transactions::CounterPhase::Rejected
        );
        assert!(
            visits
                .iter()
                .any(|v| v.control == entries[0].control && v.normal_equation.is_some())
        );
    });
}

#[test]
fn single_counter_candidate_does_not_claim_a_nonmatching_second_loop() {
    let segment = |i, step| {
        format!(
            "var n{i} = n; var g{i} = n{i} > 0; var a{i} = \"a\"; var b{i} = \"b\"; while(g{i}) {{ val x{i} = replace(&a{i}, \"x\"); val y{i} = replace(&b{i}, \"y\"); n{i} = n{i} - {step}; g{i} = n{i} > 0; }};"
        )
    };
    for (suffix, products) in [(String::new(), 1), (segment(1, 2), 0)] {
        let text = format!("fun run(n: Int) {{ {} {suffix} }}", segment(0, 1));
        let fixture = Fixture::new(&[("single-counter-candidate.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert_eq!(
            owned
                .owner_observations()
                .iter()
                .filter_map(|o| o.counter_tail_evidence_v1())
                .count(),
            products
        );
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                super::super::super::super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            assert_eq!(
                planner
                    .owner_control_probe
                    .as_ref()
                    .unwrap()
                    .iter()
                    .filter(|v| v.entering && v.counter_phase.is_some())
                    .count(),
                1
            );
        });
    }
}
