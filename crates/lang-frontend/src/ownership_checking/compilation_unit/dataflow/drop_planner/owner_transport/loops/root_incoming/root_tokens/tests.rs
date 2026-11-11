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
                assert!(writes.iter().all(|write| write.token_link.is_some() && write.token_retirement.is_some()));
            } else {
                assert!(writes.iter().all(|write| write.token_link.is_none() && write.token_retirement.is_none()));
            }
        });
    }
}
