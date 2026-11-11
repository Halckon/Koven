//! CI-only corruption of real producer snapshots. Compile locally, never execute locally.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use std::cell::RefCell;
thread_local! { static SNAPSHOT: RefCell<Option<Vec<ActionSource>>> = const { RefCell::new(None) }; }
pub(super) fn record(source: &ActionSource) {
    SNAPSHOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(snapshots) = slot.as_mut() else {
            return;
        };
        if source.nested_handoffs.is_empty() {
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
                _ => panic!("fixture must contain only actual acquisitions and live handoffs"),
            })
            .collect();
        snapshots.push(ActionSource {
            identity: source.identity.clone(),
            records,
            nested_entries: source.nested_entries.clone(),
            nested_handoffs: source.nested_handoffs.clone(),
            ..ActionSource::default()
        });
    });
}
fn capture() -> ActionSource {
    let fixture = Fixture::new(&[(
        "nested.ko",
        "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) } }",
    )]);
    SNAPSHOT.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    let _owned = fixture.shared();
    let mut snapshots = SNAPSHOT.with(|slot| slot.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    snapshots.pop().unwrap()
}
#[test]
fn ci_only_nested_lineage_rejects_wrong_entry_acquisition_and_delivery() {
    for fault in 0..13 {
        let mut source = capture();
        let unit = source.nested_handoffs[0]
            .1
            .state
            .plan
            .callable
            .source_unit();
        let index = source.nested_handoffs[0].0;
        let Record::BorrowForwardHandoff { transaction, .. } = &source.records[index] else {
            panic!("handoff")
        };
        let file = transaction.source_loan().begin_span().source_id();
        assert_eq!(source.nested_lineages(unit, file).len(), 3);
        let original = source.nested_handoffs[0].1.state.plan.leaves[0].value;
        match fault {
            0 => source.nested_handoffs[0].1.acquisition.identity = Arc::new(()),
            1 => {
                source.nested_handoffs[0].1.acquisition.index =
                    source.nested_handoffs[1].1.acquisition.index
            }
            2 => {
                source.nested_handoffs[0].1.state.path[1] =
                    source.nested_handoffs[1].1.state.path[1]
            }
            3 => {
                source.nested_handoffs[0].1.state.path[0] =
                    source.nested_handoffs[2].1.state.path[0]
            }
            4 => {
                let child = source.nested_handoffs[0].1.state.path[1].unwrap();
                source.nested_entries[child].parent = None;
            }
            5 => {
                let child = source.nested_handoffs[0].1.state.path[1].unwrap();
                source.nested_entries[child].cursor = index + 1;
            }
            6 => source.nested_handoffs[0].1.state.leaf = Some(1),
            7 => {
                let copy = source.nested_handoffs[0].clone();
                source.nested_handoffs.push(copy);
            }
            8 => source.nested_handoffs[0].1.state.path[1] = None,
            9 => source.nested_handoffs[0].1.state.plan.leaves[0].control = original,
            10 => source.nested_handoffs[0].1.state.ready_split = Some(1),
            11 => {
                source.nested_handoffs[0].1.state.plan.splits[1].split =
                    source.nested_handoffs[0].1.state.plan.splits[0].split
            }
            _ => {
                let Record::BorrowForwardHandoff { transaction, .. } = &source.records[index]
                else {
                    panic!("actual delivery")
                };
                source.records.push(Record::BorrowForwardHandoff {
                    transaction: transaction.clone(),
                    cause: None,
                    completed: None,
                });
            }
        }
        assert!(
            source
                .nested_lineages(unit, file)
                .iter()
                .all(|lineage| lineage.value() != original),
            "fault {fault}"
        );
    }
}
