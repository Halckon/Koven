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
    capture_topology(false)
}
fn capture_topology(mirror: bool) -> ActionSource {
    let text = "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) } }";
    let text = if mirror {
        text.replace("if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) }", "if (a) { return left(s) } else { if (b) { return middle(s) } else { return right(s) } }")
    } else {
        text.to_owned()
    };
    capture_text(&text)
}
fn capture_text(text: &str) -> ActionSource {
    let fixture = Fixture::new(&[("nested.ko", text)]);
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
        let original = source.nested_handoffs[0].1.state.plan.leaves()[0].value;
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
            9 => source.nested_handoffs[0].1.state.plan.leaves_mut()[0].control = original,
            10 => source.nested_handoffs[0].1.state.ready_split = Some(1),
            11 => {
                source.nested_handoffs[0].1.state.plan.splits_mut()[1].split =
                    source.nested_handoffs[0].1.state.plan.splits()[0].split
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

#[test]
fn ci_only_mirror_lineage_rejects_then_parent_and_foreign_topology() {
    use crate::ownership_checking::UnitBorrowNestedForwardTopology;
    for fault in 0..4 {
        let mut source = capture_topology(true);
        let cause = &source.nested_handoffs[1].1;
        let unit = cause.state.plan.callable.source_unit();
        let original = cause.state.plan.leaves()[1].value;
        let index = source.nested_handoffs[1].0;
        let Record::BorrowForwardHandoff { transaction, .. } = &source.records[index] else {
            panic!("handoff")
        };
        let file = transaction.source_loan().begin_span().source_id();
        assert_eq!(source.nested_lineages(unit, file).len(), 3);
        let parent = source.nested_handoffs[1].1.state.path[0].unwrap();
        match fault {
            0 => source.nested_entries[parent].then_arm = true,
            1 => {
                source.nested_entries[parent].topology = UnitBorrowNestedForwardTopology::NestedThen
            }
            2 => {
                source.nested_handoffs[1].1.state.plan.topology =
                    UnitBorrowNestedForwardTopology::NestedThen
            }
            _ => {
                source.nested_handoffs[1].1.state.path[0] =
                    source.nested_handoffs[0].1.state.path[0]
            }
        }
        assert!(
            source
                .nested_lineages(unit, file)
                .iter()
                .all(|l| l.value() != original),
            "fault {fault}"
        );
    }
}

#[test]
fn ci_only_balanced_lineage_rejects_cross_parent_and_wrong_fixed_tree() {
    use crate::ownership_checking::UnitBorrowNestedForwardTopology;
    let text = "fun id(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { if (b) { return id(s) } else { return id(s) } } else { if (c) { return id(s) } else { return id(s) } } }";
    for fault in 0..5 {
        let mut source = capture_text(text);
        let cause = source.nested_handoffs[2].1.clone();
        let unit = cause.state.plan.callable.source_unit();
        let Record::BorrowForwardHandoff { transaction, .. } =
            &source.records[source.nested_handoffs[2].0]
        else {
            panic!("handoff")
        };
        let file = transaction.source_loan().begin_span().source_id();
        assert_eq!(source.nested_lineages(unit, file).len(), 4);
        let target = cause.state.plan.leaves()[2].value;
        let parent = cause.state.path[0].unwrap();
        let child = cause.state.path[1].unwrap();
        match fault {
            0 => source.nested_entries[child].parent = source.nested_handoffs[0].1.state.path[0],
            1 => source.nested_entries[parent].then_arm = true,
            2 => {
                source.nested_handoffs[2].1.state.plan.topology =
                    UnitBorrowNestedForwardTopology::NestedElse
            }
            3 => source.nested_handoffs[2].1.state.plan.leaves_mut()[2].path[1] = Some((1, true)),
            _ => {
                source.nested_handoffs[2].1.state.plan.splits_mut()[2].split =
                    cause.state.plan.splits()[1].split
            }
        }
        assert!(
            source
                .nested_lineages(unit, file)
                .iter()
                .all(|lineage| lineage.value() != target),
            "fault {fault}"
        );
    }
}

#[test]
fn ci_only_shared_inner_lineage_binds_layout_without_merging_control_points() {
    use crate::ownership_checking::UnitBorrowNestedForwardFlagLayout;
    let text = "fun id(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return id(s) } else { return id(s) } } else { if (b) { return id(s) } else { return id(s) } } }";
    for fault in 0..4 {
        let mut source = capture_text(text);
        let cause = source.nested_handoffs[2].1.clone();
        let unit = cause.state.plan.callable.source_unit();
        let Record::BorrowForwardHandoff { transaction, .. } =
            &source.records[source.nested_handoffs[2].0]
        else {
            panic!("handoff")
        };
        let file = transaction.source_loan().begin_span().source_id();
        assert_eq!(source.nested_lineages(unit, file).len(), 4);
        let target = cause.state.plan.leaves()[2].value;
        match fault {
            0 => {
                source.nested_handoffs[2].1.state.plan.flag_layout =
                    UnitBorrowNestedForwardFlagLayout::Distinct
            }
            1 => {
                source.nested_entries[cause.state.path[1].unwrap()].flag_layout =
                    UnitBorrowNestedForwardFlagLayout::Distinct
            }
            2 => {
                source.nested_handoffs[2].1.state.plan.splits_mut()[2].flag =
                    cause.state.plan.splits()[0].flag
            }
            _ => {
                source.nested_entries[cause.state.path[1].unwrap()].split =
                    cause.state.plan.splits()[1].split
            }
        }
        assert!(
            source
                .nested_lineages(unit, file)
                .iter()
                .all(|lineage| lineage.value() != target),
            "fault {fault}"
        );
    }
}
