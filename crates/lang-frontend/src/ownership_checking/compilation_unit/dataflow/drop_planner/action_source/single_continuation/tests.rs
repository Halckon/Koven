//! CI-only fault injection over real single-survivor observations. Compile, never run locally.
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
        if source.single_regions.is_empty() {
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
                _ => {
                    panic!("real single-survivor fixture has only acquisition and handoff records")
                }
            })
            .collect();
        snapshots.push(ActionSource {
            identity: source.identity.clone(),
            records,
            single_regions: source.single_regions.clone(),
            ..ActionSource::default()
        });
    });
}
fn capture() -> ActionSource {
    let fixture = Fixture::new(&[(
        "single-ci.ko",
        "fun c0(s: String): borrow String from s = s\nfun c1(s: String): borrow String from s = s\nfun c2(s: String): borrow String from s = s\nfun c3(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { return c0(s) } else { if (b) { return c1(s) } else { if (c) { return c2(s) } } }; return c3(s) }",
    )]);
    SNAPSHOTS.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    let _owned = fixture.shared();
    let mut snapshots = SNAPSHOTS.with(|slot| slot.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    let source = snapshots.pop().unwrap();
    let regions = source.finish_single();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].coverage(), UnitLoanActionCoverage::Complete);
    assert_eq!(regions[0].paths().len(), 4);
    assert_eq!(regions[0].continuation().unwrap().resumptions().len(), 3);
    source
}
fn assert_partial(source: &ActionSource) {
    let regions = source.finish_single();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].coverage(), UnitLoanActionCoverage::Partial);
    assert!(regions[0].paths().is_empty());
    assert!(regions[0].continuation().is_none());
}
#[test]
fn ci_only_single_continuation_requires_exact_live_chain_and_tail() {
    for fault in 0..19 {
        let mut source = capture();
        let c = &mut source.single_regions[0];
        let hole = c.plan.hole;
        match fault {
            0 => c.actual[hole].token.as_mut().unwrap().outcome = Outcome::Closed,
            1 => c.actual[0].token.as_mut().unwrap().consumed = false,
            2 => {
                c.resumptions.pop();
            }
            3 => c.resumptions.swap(0, 1),
            4 => c.tail.delivery = None,
            5 => c.tail.stage = Stage::LeafLive,
            6 => c.tail_ready = false,
            7 => c.resumptions[0].survivor.node = 0,
            8 => c.resumptions[1].cursor += 1,
            9 => c.actual[hole].token.as_mut().unwrap().identity = Arc::new(()),
            10 => c.actual[hole].token.as_mut().unwrap().state.candidate = usize::MAX,
            11 => c.actual[hole].token.as_mut().unwrap().end += 1,
            12 => c.entries[c.actual[hole].entry.unwrap()].parent = None,
            13 => c.start = usize::MAX,
            14 => c.plan.nodes[0].parent = Some((0, false)),
            15 => {
                c.actual.clear();
            }
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
fn before_root_resume() -> (ActionSource, SingleResume) {
    let mut source = capture();
    let c = &mut source.single_regions[0];
    let actual = c.resumptions.pop().unwrap();
    let resume = SingleResume {
        identity: source.identity.clone(),
        candidate: 0,
        parent: actual.parent,
        survivor: actual.survivor,
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
fn ci_only_single_resume_rejects_skipping_cross_catalog_and_replay() {
    let (mut source, resume) = before_root_resume();
    let survivor = Some(resume.survivor);
    assert!(
        source
            .single_resume(resume.clone(), survivor, true)
            .unwrap()
            .tail
    );
    assert!(source.single_resume(resume, survivor, true).is_none());
    for fault in 0..7 {
        let (mut source, mut resume) = before_root_resume();
        let mut survivor = Some(resume.survivor);
        match fault {
            0 => resume.identity = Arc::new(()),
            1 => survivor = None,
            2 => resume.survivor.node = source.single_regions[0].plan.hole,
            3 => source.single_regions[0].pending = None,
            4 => {
                source.single_regions[0].actual[resume.survivor.node]
                    .token
                    .as_mut()
                    .unwrap()
                    .consumed = false
            }
            5 => {
                source.single_regions[0].actual[resume.survivor.node]
                    .token
                    .as_mut()
                    .unwrap()
                    .outcome = Outcome::Closed
            }
            _ => resume.cursor += 1,
        }
        assert!(
            source.single_resume(resume, survivor, true).is_none(),
            "fault {fault}"
        );
    }
    let (mut source, resume) = before_root_resume();
    let survivor = Some(resume.survivor);
    assert!(source.single_resume(resume, survivor, false).is_none());
}
