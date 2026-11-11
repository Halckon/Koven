//! CI-only actual producer snapshot corruptions. Compile locally; never run locally.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use std::cell::RefCell;
type Snapshot = (ActionSource, Vec<UnitLoanNestedForwardLineage>);
thread_local! { static SNAPSHOTS: RefCell<Option<Vec<Snapshot>>> = const { RefCell::new(None) }; }
pub(super) fn record(source: &ActionSource, lineages: &[UnitLoanNestedForwardLineage]) {
    SNAPSHOTS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(snapshots) = slot.as_mut() else {
            return;
        };
        if source.nested_regions.is_empty() {
            return;
        }
        let records = source
            .records
            .iter()
            .map(|r| match r {
                Record::Begin(s) => Record::Begin(s.clone()),
                Record::BorrowForwardHandoff {
                    transaction,
                    cause,
                    completed,
                } => Record::BorrowForwardHandoff {
                    transaction: transaction.clone(),
                    cause: cause.clone(),
                    completed: *completed,
                },
                _ => panic!("bounded real fixture"),
            })
            .collect();
        snapshots.push((
            ActionSource {
                identity: source.identity.clone(),
                records,
                nested_regions: source.nested_regions.clone(),
                nested_entries: source.nested_entries.clone(),
                nested_handoffs: source.nested_handoffs.clone(),
                ..ActionSource::default()
            },
            lineages.to_vec(),
        ));
    });
}
fn capture() -> Snapshot {
    capture_topology(false)
}
fn capture_topology(mirror: bool) -> Snapshot {
    let text = "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) } }";
    let text = if mirror {
        text.replace("if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) }", "if (a) { return left(s) } else { if (b) { return middle(s) } else { return right(s) } }")
    } else {
        text.to_owned()
    };
    capture_text(&text)
}
fn capture_text(text: &str) -> Snapshot {
    let fixture = Fixture::new(&[("closure.ko", text)]);
    SNAPSHOTS.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    let _owned = fixture.shared();
    let mut snapshots = SNAPSHOTS.with(|slot| slot.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    snapshots.pop().unwrap()
}
#[test]
fn ci_only_nested_closure_requires_inner_consumption_and_exact_whole_region() {
    for fault in 0..13 {
        let (mut source, mut lineages) = capture();
        assert_eq!(
            source.finish_nested_regions(&lineages)[0].coverage(),
            UnitLoanActionCoverage::Complete
        );
        match fault {
            0 => source.nested_regions[0].inner_closed = false,
            1 => source.nested_regions[0].inner_consumed = false,
            2 => source.nested_regions[0].stage = 16,
            3 => source.nested_regions[0].closed = None,
            4 => source.nested_regions[0].closed = Some(source.nested_regions[0].start + 5),
            5 => {
                lineages.pop();
            }
            6 => lineages.swap(0, 1),
            7 => lineages[0].identity = Arc::new(()),
            8 => source.nested_regions[0].entries[1] = source.nested_regions[0].entries[2],
            9 => source.nested_region_join(0, 1, true, true, true),
            10 => source.seal_nested_region(0, true, true),
            11 => {
                let callable = source.nested_regions[0].plan.callable;
                source.close_nested_callable(callable);
            }
            _ => source.nested_regions[0].start += 1,
        }
        let regions = source.finish_nested_regions(&lineages);
        assert_eq!(
            regions[0].coverage(),
            UnitLoanActionCoverage::Partial,
            "fault {fault}"
        );
        assert!(regions[0].paths().is_empty());
    }
}
#[test]
fn ci_only_nested_closure_invalidity_is_sticky_after_missing_or_reordered_event() {
    let (source, _) = capture();
    let mut c = source.nested_regions[0].clone();
    c.stage = 0;
    c.valid = true;
    c.closed = None;
    assert!(!c.advance(1, 0, c.start, true));
    assert!(!c.advance(0, 0, c.start, true));
    assert!(!c.valid);
    let mut c = source.nested_regions[0].clone();
    c.stage = 0;
    c.valid = true;
    c.closed = None;
    assert!(!c.advance(0, 0, c.start, false));
    assert!(!c.advance(0, 0, c.start, true));
}

#[test]
fn ci_only_mirror_terminal_closure_requires_its_own_topology_and_inner_consumption() {
    use crate::ownership_checking::UnitBorrowNestedForwardTopology;
    for fault in 0..6 {
        let (mut source, mut lineages) = capture_topology(true);
        assert_eq!(
            source.finish_nested_regions(&lineages)[0].coverage(),
            UnitLoanActionCoverage::Complete
        );
        assert_eq!(
            source.finish_nested_regions(&lineages)[0].topology(),
            UnitBorrowNestedForwardTopology::NestedElse
        );
        match fault {
            0 => {
                source.nested_regions[0].plan.topology = UnitBorrowNestedForwardTopology::NestedThen
            }
            1 => source.nested_regions[0].inner_consumed = false,
            2 => source.nested_regions[0].inner_closed = false,
            3 => lineages[1].topology = UnitBorrowNestedForwardTopology::NestedThen,
            4 => source.nested_regions[0].entries[0] = source.nested_regions[0].entries[3],
            _ => source.nested_region_join(0, 0, true, true, true),
        }
        let r = source.finish_nested_regions(&lineages);
        assert_eq!(
            r[0].coverage(),
            UnitLoanActionCoverage::Partial,
            "fault {fault}"
        );
        assert!(r[0].paths().is_empty());
    }
}

#[test]
fn ci_only_balanced_closure_requires_both_tokens_and_all_four_actual_paths() {
    let text = "fun first(s: String): borrow String from s = s\nfun second(s: String): borrow String from s = s\nfun third(s: String): borrow String from s = s\nfun fourth(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { if (b) { return first(s) } else { return second(s) } } else { if (c) { return third(s) } else { return fourth(s) } } }";
    for fault in 0..12 {
        let (mut source, mut lineages) = capture_text(text);
        let good = source.finish_nested_regions(&lineages);
        assert_eq!(good[0].coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(good[0].paths().len(), 4);
        assert_eq!(source.nested_regions[0].stage, 25);
        match fault {
            0 => source.nested_regions[0].inner_closed = false,
            1 => source.nested_regions[0].inner_consumed = false,
            2 => source.nested_regions[0].right_closed = false,
            3 => source.nested_regions[0].right_consumed = false,
            4 => source.nested_regions[0].entries[4] = source.nested_regions[0].entries[1],
            5 => source.nested_regions[0].stage = 24,
            6 => {
                lineages.swap(1, 2);
            }
            7 => {
                lineages.pop();
            }
            8 => source.nested_regions[0].closed = Some(source.nested_regions[0].start + 6),
            9 => source.nested_region_join(0, 2, true, true, true),
            10 => source.nested_region_join(0, 0, true, true, true),
            _ => {
                source.nested_regions[0].plan.topology = UnitBorrowNestedForwardTopology::NestedThen
            }
        }
        let bad = source.finish_nested_regions(&lineages);
        assert_eq!(
            bad[0].coverage(),
            UnitLoanActionCoverage::Partial,
            "fault {fault}"
        );
        assert!(bad[0].paths().is_empty());
    }
}

#[test]
fn ci_only_balanced_topology_with_three_leaf_storage_stays_invalid_without_indexing() {
    let (mut source, _) = capture();
    let mut plan = source.nested_regions[0].plan;
    plan.topology = UnitBorrowNestedForwardTopology::BalancedBoth;
    let id = source.begin_nested_region(plan, (0, 0), true);
    assert!(!source.nested_regions[id].valid);
    source.nested_region_arm(id, 2, true, 0, true);
    source.nested_region_return(id, 3, false, true);
    assert!(!source.nested_regions[id].valid);
}

#[test]
fn ci_only_shared_inner_closure_requires_layout_and_each_independent_path() {
    use crate::ownership_checking::UnitBorrowNestedForwardFlagLayout;
    let text = "fun id(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return id(s) } else { return id(s) } } else { if (b) { return id(s) } else { return id(s) } } }";
    for fault in 0..5 {
        let (mut source, mut lineages) = capture_text(text);
        let good = source.finish_nested_regions(&lineages);
        assert_eq!(good[0].coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(
            good[0].flag_layout(),
            UnitBorrowNestedForwardFlagLayout::SharedInner
        );
        assert_eq!(good[0].paths().len(), 4);
        match fault {
            0 => {
                source.nested_regions[0].plan.flag_layout =
                    UnitBorrowNestedForwardFlagLayout::Distinct
            }
            1 => lineages[2].flag_layout = UnitBorrowNestedForwardFlagLayout::Distinct,
            2 => source.nested_regions[0].right_consumed = false,
            3 => {
                lineages.remove(2);
            }
            _ => source.nested_regions[0].entries[4] = source.nested_regions[0].entries[1],
        }
        let bad = source.finish_nested_regions(&lineages);
        assert_eq!(
            bad[0].coverage(),
            UnitLoanActionCoverage::Partial,
            "fault {fault}"
        );
        assert!(bad[0].paths().is_empty());
    }
}
