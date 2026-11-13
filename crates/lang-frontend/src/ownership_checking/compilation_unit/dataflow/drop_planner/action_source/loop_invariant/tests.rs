//! CI-only corruption of actual captured candidates. Compile locally; execute in CI only.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use std::cell::Cell;
thread_local! { static FAULT: Cell<Option<u8>> = const { Cell::new(None) }; }
pub(in super::super) fn mutate(mut source: ActionSource) -> ActionSource {
    let Some(fault) = FAULT.get() else {
        return source;
    };
    let Some(candidate) = source.loop_invariants.first_mut() else {
        return source;
    };
    match fault {
        0 => candidate.token.stage = 0,
        1 => candidate.end = None,
        2 => candidate.token.begin.identity = Arc::new(()),
        3 => candidate.token.handoff.identity = Arc::new(()),
        4 => candidate.token.identity = Arc::new(()),
        5 => candidate.end.as_mut().unwrap().identity = Arc::new(()),
        6 => candidate.token.cursors[2] = usize::MAX,
        7 => source.loop_returns.clear(),
        8 => {
            source.loop_returns.remove(0);
        }
        9 => source.loop_returns.swap(0, 1),
        10 => source.loop_returns[0].begin.identity = Arc::new(()),
        11 => source.loop_returns[0].cursor = usize::MAX,
        12 => candidate.token.plan.consumers.swap(0, 1),
        13 => {
            let returned = &source.loop_returns[0];
            source.loop_returns.push(Returned {
                loan: returned.loan.clone(),
                begin: returned.begin.clone(),
                cursor: returned.cursor,
            });
        }
        14 => {
            source
                .records
                .push(Record::Begin(source.loop_returns[0].loan.clone()));
        }
        _ => {
            let at = candidate.token.cursors[1];
            source
                .records
                .insert(at, Record::Begin(candidate.token.source.clone()));
        }
    }
    source
}
#[test]
fn ci_only_loop_lineage_rejects_missing_foreign_replayed_and_extra_actual_evidence() {
    for fault in std::iter::once(None).chain((0..16).map(Some)) {
        FAULT.set(fault);
        let fixture = Fixture::new(&[(
            "loop.ko",
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit {val source=\"value\";borrow val item=view(source);while(flag){consume(item);consume(item)}}",
        )]);
        let shared = fixture.shared();
        let catalog = &shared.loan_action_observations()[0];
        assert_eq!(
            catalog.loop_invariant_lineages().len(),
            usize::from(fault.is_none()),
            "fault {fault:?}"
        );
        FAULT.set(None);
    }
}
