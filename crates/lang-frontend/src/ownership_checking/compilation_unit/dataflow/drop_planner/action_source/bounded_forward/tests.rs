//! CI-only corruptions of the actual bounded-tree producer. Compile locally, never execute locally.
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
        if source.bounded_regions.is_empty() {
            return;
        }
        let records = source
            .records
            .iter()
            .map(|record| match record {
                Record::Begin(source) => Record::Begin(source.clone()),
                Record::BorrowForwardHandoff {
                    transaction,
                    cause,
                    completed,
                } => Record::BorrowForwardHandoff {
                    transaction: transaction.clone(),
                    cause: cause.clone(),
                    completed: *completed,
                },
                _ => panic!("real bounded fixture has only acquisitions and live handoffs"),
            })
            .collect();
        snapshots.push(ActionSource {
            identity: source.identity.clone(),
            records,
            bounded_regions: source.bounded_regions.clone(),
            ..ActionSource::default()
        });
    });
}
fn capture() -> ActionSource {
    let fixture = Fixture::new(&[(
        "bounded-ci.ko",
        "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun last(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { if (b) { if (c) { return left(s) } else { return middle(s) } } else { return right(s) } } else { return last(s) } }",
    )]);
    SNAPSHOTS.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    let _owned = fixture.shared();
    let mut snapshots = SNAPSHOTS.with(|slot| slot.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    let source = snapshots.pop().unwrap();
    let regions = source.finish_bounded();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].coverage(), UnitLoanActionCoverage::Complete);
    assert_eq!(regions[0].paths().len(), 4);
    source
}
fn assert_partial(source: &ActionSource) {
    let regions = source.finish_bounded();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].coverage(), UnitLoanActionCoverage::Partial);
    assert!(regions[0].paths().is_empty());
}

#[test]
fn ci_only_bounded_final_requires_all_actual_tokens_entries_and_delivery() {
    for fault in 0..17 {
        let mut source = capture();
        let c = &mut source.bounded_regions[0];
        let leaf = c.plan.leaves[0];
        let entry = c.actual[leaf].entry.unwrap();
        match fault {
            0 => c.actual[leaf].token = None,
            1 => c.actual[leaf].token.as_mut().unwrap().identity = Arc::new(()),
            2 => c.actual[leaf].token.as_mut().unwrap().candidate = usize::MAX,
            3 => c.actual[leaf].token.as_mut().unwrap().consumed = false,
            4 => c.actual[0].token.as_mut().unwrap().consumed = true,
            5 => c.entries[entry].parent = None,
            6 => c.stack.push(0),
            7 => c.sealed = false,
            8 => c.closed = None,
            9 => c.start += 1,
            10 => c.actual[leaf].delivery = None,
            11 => c.actual[leaf].token.as_mut().unwrap().end += 1,
            12 => c.entries[entry].cursor += 1,
            13 => {
                source.records.pop();
            }
            14 => {
                c.start = usize::MAX;
            }
            15 => {
                let begin = c.actual[leaf].begin;
                let Record::Begin(loan) = &source.records[begin] else {
                    panic!("actual begin");
                };
                source.records.push(Record::Begin(loan.clone()));
            }
            _ => {
                let index = c.actual[leaf].delivery.unwrap().1;
                let Record::BorrowForwardHandoff {
                    transaction,
                    cause,
                    completed,
                } = &source.records[index]
                else {
                    panic!("actual handoff");
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

#[test]
fn ci_only_bounded_join_consumes_exact_children_once() {
    for fault in 0..8 {
        let mut source = capture();
        let c = &mut source.bounded_regions[0];
        let Kind::Split {
            split, children, ..
        } = c.plan.nodes[0].kind
        else {
            panic!("root split");
        };
        c.sealed = false;
        c.closed = None;
        c.stack = vec![0];
        c.actual[0].stage = Stage::ReadyJoin;
        c.actual[0].token = None;
        for child in children {
            c.actual[child].token.as_mut().unwrap().consumed = false;
        }
        match fault {
            0 => c.actual[children[0]].token = None,
            1 => c.actual[children[0]].token = c.actual[children[1]].token.clone(),
            2 => c.actual[children[0]].token.as_mut().unwrap().identity = Arc::new(()),
            3 => c.actual[children[0]].token.as_mut().unwrap().candidate = 1,
            4 => c.actual[children[0]].token.as_mut().unwrap().consumed = true,
            5 => c.actual[children[1]].token.as_mut().unwrap().begin += 1,
            6 => {
                let entry = c.actual[children[0]].entry.unwrap();
                c.entries[entry].parent = Some(entry);
            }
            _ => {}
        }
        let state = BoundedState {
            candidate: 0,
            node: 0,
            entry: None,
        };
        source.bounded_join(state, split, true, true, true);
        assert_eq!(source.bounded_regions[0].valid, fault == 7, "fault {fault}");
        if fault == 7 {
            assert!(
                source.bounded_regions[0].actual[children[0]]
                    .token
                    .as_ref()
                    .unwrap()
                    .consumed
            );
            assert!(
                source.bounded_regions[0].actual[children[1]]
                    .token
                    .as_ref()
                    .unwrap()
                    .consumed
            );
        }
        source.bounded_join(state, split, true, true, true);
        assert!(!source.bounded_regions[0].valid);
        assert_partial(&source);
    }
}

#[test]
fn ci_only_bounded_malformed_plans_fail_closed_without_unbounded_traversal() {
    for fault in 0..7 {
        let mut source = capture();
        let plan = &mut source.bounded_regions[0].plan;
        match fault {
            0 => {
                let Kind::Split { children, .. } = &mut plan.nodes[0].kind else {
                    panic!("split");
                };
                children[0] = usize::MAX;
            }
            1 => {
                let Kind::Split { children, .. } = &mut plan.nodes[0].kind else {
                    panic!("split");
                };
                children[0] = 0;
            }
            2 => plan.nodes[plan.leaves[0]].depth = 4,
            3 => plan.split_count = 8,
            4 => plan.leaves.push(plan.leaves[0]),
            5 => plan.nodes[0].parent = Some((0, true)),
            _ => plan.nodes[plan.leaves[0]].parent = Some((usize::MAX, true)),
        }
        assert!(!plan.valid_structure());
        assert_partial(&source);
    }
}
