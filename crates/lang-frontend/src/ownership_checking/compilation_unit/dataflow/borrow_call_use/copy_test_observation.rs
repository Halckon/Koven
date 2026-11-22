//! Records the exact checker mutation while traversing the original source.
use crate::{
    name_resolution::SourceUnitKey,
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitLoanTarget,
        compilation_unit::{
            dataflow::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget},
            formed_tests::Fixture,
        },
    },
    type_checking::UnitExpressionId,
};
use std::cell::RefCell;

#[derive(Debug)]
enum Event {
    CopyRemoving {
        active: ActiveLoan,
        outer_call: UnitExpressionId,
        point: UnitDropPoint,
        source_argument: UnitExpressionId,
    },
    ExclusiveInserted {
        active: ActiveLoan,
        argument: UnitExpressionId,
    },
}

thread_local! {
    static EVENTS: RefCell<Option<Vec<Event>>> = const { RefCell::new(None) };
}

fn record(make: impl FnOnce() -> Event) {
    EVENTS.with_borrow_mut(|slot| {
        if let Some(events) = slot {
            events.push(make());
        }
    });
}

struct Probe;
impl Probe {
    fn start() -> Self {
        EVENTS.with_borrow_mut(|slot| {
            assert!(slot.is_none(), "one scoped probe per test thread");
            *slot = Some(Vec::new());
        });
        Self
    }

    fn finish(self) -> Vec<Event> {
        EVENTS.with_borrow_mut(|slot| slot.take().unwrap())
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        EVENTS.with_borrow_mut(|slot| *slot = None);
    }
}

pub(in crate::ownership_checking::compilation_unit::dataflow) fn copy_removing(
    loan: &ActiveLoan,
    outer_call: UnitExpressionId,
    argument: UnitExpressionId,
    source_argument: UnitExpressionId,
) {
    record(|| Event::CopyRemoving {
        active: loan.clone(),
        outer_call,
        point: UnitDropPoint::AfterExpression(argument),
        source_argument,
    });
}

pub(in crate::ownership_checking::compilation_unit::dataflow) fn exclusive_inserted(
    loan: &ActiveLoan,
    argument: UnitExpressionId,
) {
    record(|| Event::ExclusiveInserted {
        active: loan.clone(),
        argument,
    });
}

#[test]
fn nullable_copy_ends_nested_source_before_the_next_operand() {
    let text = "fun view(source: Int): borrow Int from source = source\nfun observe(value: Int?, inout changed: Int): Unit { changed = 9 }\nfun run() { var n: Int = 7; observe(value = ((view(n))), changed = &n) }";
    for rooted in [false, true] {
        let fixture = Fixture::standard_source("nested_borrow.ko", text, rooted);
        let [entry] = fixture.names.names().index().source_units() else {
            panic!("one original source");
        };
        assert_eq!(fixture.sources.source_text(entry.source_id()), Ok(text));
        match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "nested_borrow.ko");
            }
            _ => panic!("requested Unit layout"),
        }
        let baseline = fixture.shared();
        let probe = Probe::start();
        let unit = fixture.shared();
        let events = probe.finish();

        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        assert_eq!(baseline.borrow_results(), unit.borrow_results());
        assert_eq!(baseline.loans(), unit.loans());
        assert_eq!(baseline.drops(), unit.drops());

        let [usage] = unit.borrow_results().call_uses() else {
            panic!("one copy conversion");
        };
        let endpoint = UnitDropPoint::AfterExpression(usage.argument());
        assert_eq!(usage.end_point(), endpoint);
        let source = usage.source_loan();
        let [
            Event::CopyRemoving {
                active: retired,
                outer_call,
                point,
                source_argument,
            },
            Event::ExclusiveInserted {
                active: following,
                argument,
            },
        ] = events.as_slice()
        else {
            panic!("one removal before the one next inout insertion, got {events:?}");
        };
        let source_unit = fixture.source(0);
        let expression =
            |spelling| UnitExpressionId::new(source_unit, fixture.expression(0, spelling));
        assert_eq!(source.call(), expression("view(n)"));
        assert_eq!(usage.argument(), expression("((view(n)))"));
        assert_eq!(*outer_call, usage.call());
        assert_eq!(*point, endpoint);
        assert_eq!(*source_argument, source.argument());
        assert_eq!(retired.owner, ActiveLoanOwner::Call(source.call()));
        assert_eq!(retired.kind, LoanKind::Shared);
        let UnitLoanTarget::Place(origin) = usage.origin() else {
            panic!("named source place");
        };
        assert_eq!(retired.target, ActiveLoanTarget::Place(origin.clone()));
        let call = fixture.parsed[0]
            .ast()
            .expressions()
            .get(usage.call().expression())
            .unwrap();
        let crate::parser::Expression::Call { arguments, .. } = call.payload() else {
            panic!("actual outer call");
        };
        let [copied_operand, next_operand] = arguments.as_slice() else {
            panic!("the copy and inout are the two arguments in source order");
        };
        assert_eq!(copied_operand.value, usage.argument().expression());
        assert_eq!(
            *argument,
            UnitExpressionId::new(source_unit, next_operand.value)
        );
        assert_eq!(fixture.sources.slice(next_operand.span), Ok("changed = &n"));
        assert_eq!(fixture.sources.slice(following.origin), Ok("&"));
        assert_eq!(following.owner, ActiveLoanOwner::Call(usage.call()));
        assert_eq!(following.kind, LoanKind::Exclusive);
        assert_eq!(following.target, retired.target);
        let original_loan = unit
            .loans()
            .iter()
            .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
            .expect("exact original inner loan");
        assert_eq!(original_loan.begin_span(), retired.origin);
        let next_loan = unit
            .loans()
            .iter()
            .find(|loan| loan.call() == usage.call() && loan.argument() == *argument)
            .expect("exact next inout loan");
        assert_eq!(next_loan.begin_span(), following.origin);
        assert!(unit.loans().iter().any(|loan| {
            loan.call() == usage.call()
                && loan.argument() == usage.argument()
                && loan.kind() == LoanKind::Shared
                && loan.target() == &UnitLoanTarget::Temporary(usage.argument())
        }));
        assert!(unit.borrow_call_transports().is_empty());
        assert!(unit.validate().is_ok());
    }
}
