//! Private snapshots of the actual checker transition, not a published lifecycle product.
use std::cell::RefCell;

use super::flow::{ActiveLoanOwner, ActiveLoanTarget, State};
use crate::type_checking::UnitExpressionId;

type LifecycleSnapshot = (&'static str, UnitExpressionId, State);
type SnapshotBuffer = Option<Vec<LifecycleSnapshot>>;

thread_local! {
    static SNAPSHOTS: RefCell<SnapshotBuffer> = const { RefCell::new(None) };
}

pub(super) fn record(phase: &'static str, call: UnitExpressionId, state: &State) {
    SNAPSHOTS.with(|slot| {
        if let Some(snapshots) = slot.borrow_mut().as_mut() {
            snapshots.push((phase, call, state.clone()));
        }
    });
}

#[test]
fn shared_callback_loans_survive_last_callee_use_until_call_finishes() {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    const SOURCE: &str = "fun run(): Unit {\nvar a = \"old\"\nval old = replace(&a, \"new\")\nval callback = { a + old }\ncallback()\n}";
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SNAPSHOTS.with(|slot| *slot.borrow_mut() = None);
        }
    }
    SNAPSHOTS.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Vec::new());
    });
    let reset = Reset;
    let observed = fixture.shared();
    let snapshots = SNAPSHOTS.with(|slot| slot.borrow_mut().take().unwrap());
    drop(reset);
    let ordinary = fixture.shared();
    assert!(observed.diagnostics().is_empty());
    assert!(observed.clone().validate().is_ok());
    assert_eq!(observed, ordinary);
    let call = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "callback()"));
    let closure = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{ a + old }"));
    let snapshots = snapshots
        .iter()
        .filter(|(_, id, _)| *id == call)
        .collect::<Vec<_>>();
    assert_eq!(
        snapshots
            .iter()
            .map(|(phase, _, _)| *phase)
            .collect::<Vec<_>>(),
        ["held", "before_finish", "after_finish"]
    );
    let held = &snapshots[0].2;
    let before = &snapshots[1].2;
    let after = &snapshots[2].2;
    let loans = |state: &State| {
        state
            .loans
            .iter()
            .filter(|loan| loan.owner == ActiveLoanOwner::Closure(closure))
            .cloned()
            .collect::<Vec<_>>()
    };
    let active = loans(held);
    assert_eq!(active.len(), 2);
    assert!(
        held.pending_closure_calls
            .get(&call)
            .unwrap()
            .contains(&closure)
    );
    assert!(
        active
            .iter()
            .all(|loan| loan.kind == super::LoanKind::Shared && !loan.reserved)
    );
    for name in ["a", "old"] {
        let root = fixture.symbol(0, name);
        assert_eq!(active.iter().filter(|loan| matches!(&loan.target, ActiveLoanTarget::Place(place) if place.root() == root)).count(), 1);
    }
    assert_eq!(loans(before), active);
    // Last callee use has retired the binding, but the pending call still holds its loans.
    assert!(!before.closures.contains_key(&fixture.symbol(0, "callback")));
    assert!(
        before
            .pending_closure_calls
            .get(&call)
            .unwrap()
            .contains(&closure)
    );
    assert!(!after.pending_closure_calls.contains_key(&call));
    assert!(loans(after).is_empty());
}
