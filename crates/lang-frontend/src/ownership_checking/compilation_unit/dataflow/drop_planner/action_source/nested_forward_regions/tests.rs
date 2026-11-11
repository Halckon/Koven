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
    let fixture = Fixture::new(&[(
        "closure.ko",
        "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) } }",
    )]);
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
