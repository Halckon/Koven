//! Exact semantic-state observations; no interpretation of the separate planner queue.
use std::cell::RefCell;

use super::super::{ActiveLoan, ActiveLoanOwner};
use crate::{
    ownership_checking::compilation_unit::formed_tests::Fixture,
    type_checking::{UnitExpressionId, UnitStatementId},
};

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Call(ActiveLoan, bool),
    Handoff(UnitExpressionId, ActiveLoan, ActiveLoan),
    BindingEnd(UnitStatementId, ActiveLoan),
}

thread_local! {
    static EVENTS: RefCell<Option<Vec<Event>>> = const { RefCell::new(None) };
}

fn capture<T>(run: impl FnOnce() -> T) -> (T, Vec<Event>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            EVENTS.with_borrow_mut(|events| *events = None);
        }
    }
    EVENTS.with_borrow_mut(|events| {
        assert!(events.is_none());
        *events = Some(Vec::new());
    });
    let _reset = Reset;
    let value = run();
    let events = EVENTS.with_borrow_mut(|events| events.take().unwrap());
    (value, events)
}

pub(in super::super) fn before(loans: &[ActiveLoan], owner: ActiveLoanOwner) -> Vec<ActiveLoan> {
    EVENTS.with_borrow(|events| {
        if events.is_some() {
            loans
                .iter()
                .filter(|loan| loan.owner == owner)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    })
}

pub(in super::super) fn after_call(before: Vec<ActiveLoan>, loans: &[ActiveLoan]) {
    EVENTS.with_borrow_mut(|events| {
        if let Some(events) = events {
            events.extend(before.into_iter().map(|loan| {
                let retained = loans.contains(&loan);
                Event::Call(loan, retained)
            }));
        }
    });
}

pub(in super::super) fn handoff(
    argument: UnitExpressionId,
    before: ActiveLoan,
    after: &ActiveLoan,
) {
    EVENTS.with_borrow_mut(|events| {
        if let Some(events) = events {
            events.push(Event::Handoff(argument, before, after.clone()));
        }
    });
}

pub(in super::super) fn after_binding_end(
    before: Vec<ActiveLoan>,
    loans: &[ActiveLoan],
    end: UnitStatementId,
) {
    EVENTS.with_borrow_mut(|events| {
        if let Some(events) = events {
            events.extend(
                before
                    .into_iter()
                    .filter(|loan| !loans.contains(loan))
                    .map(|loan| Event::BindingEnd(end, loan)),
            );
        }
    });
}

#[test]
fn original_metadata_alias_keeps_semantic_loan_until_reverse_binding_ends() {
    for element in ["Int", "String", "Item"] {
        let text = format!(
            "class Item(val value: Int) {{}}\nfun alias(source: View<{element}>): borrow View<{element}> from source = source\nfun observe(source: View<{element}>): Unit {{}}\nfun run(source: View<{element}>): Unit {{ borrow val parent = alias(source); borrow val child = alias(parent); observe(child); observe(parent) }}"
        );
        for rooted in [false, true] {
            let fixture = Fixture::standard_source("range-loans.ko", &text, rooted);
            let (owned, events) = capture(|| fixture.shared());
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
            assert!(owned.clone().validate().is_ok());
            let [parent, child] = owned.borrow_results().bindings() else {
                panic!("two original bindings");
            };
            assert!(
                parent.parent().is_none(),
                "original source is not a borrow binding"
            );
            assert_eq!(child.parent(), Some(parent.binding()));
            assert_eq!(child.origin(), parent.origin());
            assert!(owned.drops().is_empty(), "borrow metadata owns no payload");
            let [child_end, parent_end] = owned.borrow_results().ends() else {
                panic!("exact reverse endpoint pair");
            };
            assert_eq!(child_end.binding(), child.binding());
            assert_eq!(parent_end.binding(), parent.binding());
            let indexed = [
                removal_index(&owned, &events, parent, parent_end),
                removal_index(&owned, &events, child, child_end),
            ];
            assert!(indexed[1] < indexed[0], "child ends before parent");
            // An active observer must also see both ordinary calls release their own loans.
            let ordinary = ["observe(child)", "observe(parent)"]
                .map(|text| UnitExpressionId::new(fixture.source(0), fixture.expression(0, text)));
            let actual: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    Event::Call(loan, false) => match loan.owner {
                        ActiveLoanOwner::Call(call) => Some(call),
                        _ => panic!("non-call loan in a normal call snapshot"),
                    },
                    _ => None,
                })
                .collect();
            assert_eq!(actual, ordinary);
        }
    }
}

fn removal_index(
    owned: &crate::ownership_checking::CompilationUnitOwnership,
    events: &[Event],
    binding: &crate::ownership_checking::BorrowBindingFact<
        UnitExpressionId,
        crate::name_resolution::UnitSymbolId,
        crate::ownership_checking::UnitLoanTarget,
    >,
    end: &crate::ownership_checking::BorrowBindingEndFact<
        crate::name_resolution::UnitSymbolId,
        crate::ownership_checking::UnitDropPoint,
    >,
) -> usize {
    let source = binding.source_loan().unwrap();
    let actual = owned
        .loans()
        .iter()
        .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
        .unwrap();
    assert_eq!(actual.target(), binding.origin());
    assert_eq!(source.call().source_unit(), binding.binding().source_unit());
    let found = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            Event::Handoff(argument, before, after) if *argument == source.argument() => {
                assert_eq!(before.owner, ActiveLoanOwner::Call(source.call()));
                assert_eq!(before.origin, actual.begin_span());
                let mut expected = ActiveLoan::clone(before);
                expected.owner = ActiveLoanOwner::BorrowBinding(binding.binding());
                assert_eq!(&expected, after);
                Some((index, before, after))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(handoff, before, after)] = found.as_slice() else {
        panic!("exactly one actual semantic handoff");
    };
    let retained = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Event::Call(loan, retained) if loan == *before => {
                assert!(*retained, "the semantic loan did not end with its producer");
                Some(i)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [normal_return] = retained.as_slice() else {
        panic!("exactly one completed source call");
    };
    let crate::ownership_checking::UnitDropPoint::AfterStatement(statement) = end.point() else {
        panic!("exact normal last use");
    };
    let removal = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Event::BindingEnd(actual_end, loan) if loan == *after => {
                assert_eq!(*actual_end, statement);
                Some(i)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [removed] = removal.as_slice() else {
        panic!("exactly one actual removal of that source loan");
    };
    assert!(normal_return < handoff && handoff < removed);
    *removed
}

#[test]
fn original_named_range_receivers_preserve_each_semantic_loan_until_its_own_end() {
    const RECEIVERS: &str = "borrow fun <T> List<T>.prefix(count: Int): View<T> from this = rangeView(this, 0, count)\nborrow fun <T> View<T>.prefix(count: Int): View<T> from this = rangeView(this, 0, count)\n";
    for (element, value) in [
        ("String", "\"kept\""),
        ("Item", "Item(1)"),
        ("Resource", "Resource()"),
    ] {
        let text = format!(
            "{RECEIVERS}class Item(val value: Int) {{}}\nclass Resource {{ deinit() {{}} }}\nfun alias(source: View<{element}>): borrow View<{element}> from source = source\nfun inspect(source: View<{element}>): Unit {{}}\nfun consume(own source: List<{element}>): Unit {{}}\nfun run(): Unit {{ val root = listOf({value}); borrow val parent = root.prefix(1); borrow val child = parent.prefix(1); borrow val metadata = alias(child); inspect(metadata); inspect(parent); consume(root) }}"
        );
        for rooted in [false, true] {
            let fixture = Fixture::range_receiver_source("receiver.ko", &text, rooted);
            let (owned, events) = capture(|| fixture.shared());
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
            assert!(owned.clone().validate().is_ok());
            assert_eq!(
                fixture.sources.source_text(fixture.parsed[0].source_id()),
                Ok(text.as_str())
            );
            let [parent, child, metadata] = owned.borrow_results().bindings() else {
                panic!("three original semantic bindings");
            };
            assert!(parent.parent().is_none());
            assert!(child.parent().is_none(), "new descriptor remains root-flat");
            assert_eq!(metadata.parent(), Some(child.binding()));
            assert_eq!(parent.origin(), child.origin());
            assert_eq!(parent.origin(), metadata.origin());
            let [metadata_end, child_end, parent_end] = owned.borrow_results().ends() else {
                panic!("three original semantic ends");
            };
            assert_eq!(metadata_end.binding(), metadata.binding());
            assert_eq!(child_end.binding(), child.binding());
            assert_eq!(parent_end.binding(), parent.binding());
            let metadata_removed = removal_index(&owned, &events, metadata, metadata_end);
            let child_removed = removal_index(&owned, &events, child, child_end);
            let parent_removed = removal_index(&owned, &events, parent, parent_end);
            assert!(metadata_removed < child_removed && child_removed < parent_removed);
        }
    }
}

mod named_range_exhaustion;
