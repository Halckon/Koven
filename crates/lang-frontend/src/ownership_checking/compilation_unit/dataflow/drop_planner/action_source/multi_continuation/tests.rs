//! CI-only corruption tests over actual multi-survivor merges. Compile locally, never execute.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use std::cell::RefCell;
thread_local! { static SNAPSHOTS: RefCell<Option<Vec<ActionSource>>> = const { RefCell::new(None) }; }
pub(super) fn record(source: &ActionSource) {
    SNAPSHOTS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(snapshots) = slot.as_mut() else {
            return;
        };
        if source.multi_regions.is_empty() {
            return;
        }
        let records = source
            .records
            .iter()
            .map(|record| match record {
                Record::Begin(loan) => Record::Begin(loan.clone()),
                Record::BorrowForwardHandoff {
                    transaction,
                    cause,
                    completed,
                } => Record::BorrowForwardHandoff {
                    transaction: transaction.clone(),
                    cause: cause.clone(),
                    completed: *completed,
                },
                _ => panic!("actual multi fixture records only acquisitions and handoffs"),
            })
            .collect();
        snapshots.push(ActionSource {
            identity: source.identity.clone(),
            records,
            multi_regions: source.multi_regions.clone(),
            ..ActionSource::default()
        });
    });
}
fn capture() -> ActionSource {
    let fixture = Fixture::new(&[(
        "multi-ci.ko",
        "fun c0(s: String): borrow String from s = s\nfun c1(s: String): borrow String from s = s\nfun c2(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { if (b) { return c0(s) } } else { if (c) { return c1(s) } }; return c2(s) }",
    )]);
    SNAPSHOTS.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    let _owned = fixture.shared();
    let mut snapshots = SNAPSHOTS.with(|slot| slot.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    let source = snapshots.pop().unwrap();
    let regions = source.finish_multi();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].coverage(), UnitLoanActionCoverage::Complete);
    assert_eq!(regions[0].terminal_paths().len(), 2);
    assert_eq!(regions[0].origins().len(), 2);
    assert!(regions[0].tail().is_some());
    source
}
fn assert_partial(source: &ActionSource) {
    let regions = source.finish_multi();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].coverage(), UnitLoanActionCoverage::Partial);
    assert!(regions[0].terminal_paths().is_empty());
    assert!(regions[0].origins().is_empty());
    assert!(regions[0].joins().is_empty());
    assert!(regions[0].tail().is_none());
}
#[test]
fn ci_only_multi_final_requires_disjoint_complete_origins_and_one_actual_tail() {
    for fault in 0..19 {
        let mut source = capture();
        let c = &mut source.multi_regions[0];
        let hole = c.plan.holes[0];
        match fault {
            0 => c.actual[hole].token.as_mut().unwrap().outcome = Outcome::Closed,
            1 => {
                c.actual[hole].token.as_mut().unwrap().outcome = Outcome::Continues {
                    origins: vec![0, 1],
                }
            }
            2 => c.resumptions.last_mut().unwrap().incoming.swap(0, 1),
            3 => c.resumptions.last_mut().unwrap().origins = vec![0, 0],
            4 => {
                c.resumptions.pop();
            }
            5 => c.resumptions.swap(0, 1),
            6 => c.resumptions.last_mut().unwrap().survivors.reverse(),
            7 => c.tail.delivery = None,
            8 => c.tail.stage = Stage::LeafLive,
            9 => c.actual[0].token.as_mut().unwrap().consumed = false,
            10 => c.actual[hole].token.as_mut().unwrap().identity = Arc::new(()),
            11 => c.actual[hole].token.as_mut().unwrap().state.candidate = usize::MAX,
            12 => c.start = usize::MAX,
            13 => c.actual.clear(),
            14 => c.plan.holes.swap(0, 1),
            15 => c.entries[c.actual[hole].entry.unwrap()].parent = None,
            16 => {
                source.records.pop();
            }
            17 => {
                let Record::Begin(loan) = &source.records[c.tail.begin] else {
                    panic!("actual tail begin")
                };
                source.records.push(Record::Begin(loan.clone()));
            }
            _ => {
                let Record::BorrowForwardHandoff {
                    transaction,
                    cause,
                    completed,
                } = &source.records[c.tail.begin + 1]
                else {
                    panic!("actual tail handoff")
                };
                source.records.push(Record::BorrowForwardHandoff {
                    transaction: transaction.clone(),
                    cause: cause.clone(),
                    completed: *completed,
                });
            }
        }
        assert_partial(&source);
    }
}
fn before_root_resume() -> (ActionSource, MultiResume) {
    let mut source = capture();
    let c = &mut source.multi_regions[0];
    let actual = c.resumptions.pop().unwrap();
    let resume = MultiResume {
        identity: source.identity.clone(),
        candidate: 0,
        parent: actual.parent,
        survivors: actual.survivors,
        incoming: actual.incoming,
        origins: actual.origins,
        cursor: actual.cursor,
    };
    source.records.truncate(resume.cursor);
    c.sealed = false;
    c.closed = None;
    c.stack = vec![0];
    c.tail_ready = false;
    c.tail = fresh(resume.cursor);
    c.actual[0].stage = Stage::PendingResume;
    c.actual[0].token = None;
    c.pending = Some(resume.clone());
    (source, resume)
}
#[test]
fn ci_only_multi_resume_requires_actual_ordered_merge_inputs_not_union_alone() {
    let (mut source, resume) = before_root_resume();
    let merged = resume.survivors.clone();
    assert_eq!(merged.len(), 2);
    assert!(
        source
            .multi_resume(resume.clone(), Some(&merged), true)
            .unwrap()
            .tail
    );
    assert!(source.multi_resume(resume, Some(&merged), true).is_none());
    for fault in 0..9 {
        let (mut source, mut resume) = before_root_resume();
        let mut merged = resume.survivors.clone();
        match fault {
            0 => merged.reverse(),
            1 => {
                merged.pop();
            }
            2 => merged[1] = merged[0],
            3 => resume.identity = Arc::new(()),
            4 => resume.incoming.swap(0, 1),
            5 => resume.origins = vec![0, 0],
            6 => source.multi_regions[0].pending = None,
            7 => {
                source.multi_regions[0].actual[merged[1].node]
                    .token
                    .as_mut()
                    .unwrap()
                    .outcome = Outcome::Continues { origins: vec![0] }
            }
            _ => resume.cursor += 1,
        }
        assert!(
            source.multi_resume(resume, Some(&merged), true).is_none(),
            "fault {fault}"
        );
    }
    let (mut source, resume) = before_root_resume();
    assert!(source.multi_resume(resume, None, true).is_none());
    let (mut source, resume) = before_root_resume();
    let merged = resume.survivors.clone();
    assert!(source.multi_resume(resume, Some(&merged), false).is_none());
}
