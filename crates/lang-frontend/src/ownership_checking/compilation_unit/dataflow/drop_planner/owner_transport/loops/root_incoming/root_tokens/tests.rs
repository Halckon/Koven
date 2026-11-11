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
        let text = format!(
            "fun run(n: Int, flag: Boolean) {{ var remaining = n; val other = n; var again = flag; var a = \"a\"; var b = \"b\"; while(again) {{ val x = replace(&a, \"x\"); val y = replace(&b, \"y\"); {tail} }} }}"
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
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                super::super::super::super::super::super::liveness::build(checker, true).unwrap(),
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
                accepted,
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
