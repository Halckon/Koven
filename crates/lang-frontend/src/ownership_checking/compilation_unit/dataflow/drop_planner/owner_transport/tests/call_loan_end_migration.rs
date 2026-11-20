//! Preserved SPEC-0027 sources; check actual token removal, not a descriptor query.
use super::super::origin_probe::Event;
use super::*;
use crate::name_resolution::SourceUnitKey;
use crate::ownership_checking::{
    CompilationUnitOwnership, LoanKind, OwnershipBindingKind, UnitDropPoint, UnitLoanFact,
    UnitLoanTarget,
};

#[test]
fn parameter_bindings_and_successful_call_loans_are_queryable() {
    let text = "class Resource {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun exercise(own owned: Resource, shared: Resource, inout exclusive: Resource): Unit {\n\
                    val first = inspect(owned)\n\
                    val second = inspect(shared)\n\
                    val third = mutate(&exclusive)\n\
                }";
    for rooted in [false, true] {
        let fixture = ownership_fixture(text, rooted);
        let owned = fixture.shared();
        let kinds: Vec<_> = owned
            .bindings()
            .iter()
            .map(|binding| binding.kind())
            .collect();
        assert_eq!(
            kinds,
            [
                OwnershipBindingKind::Shared,
                OwnershipBindingKind::Exclusive,
                OwnershipBindingKind::Owned,
                OwnershipBindingKind::Shared,
                OwnershipBindingKind::Exclusive,
            ]
        );
        assert_calls(
            &fixture,
            owned,
            &[
                ("inspect(owned)", LoanKind::Shared),
                ("inspect(shared)", LoanKind::Shared),
                ("mutate(&exclusive)", LoanKind::Exclusive),
            ],
        );
    }
}

#[test]
fn nested_argument_loan_ends_at_its_own_call_before_outer_inout() {
    let text = "class Resource {}\n\
                fun read(item: Resource): Int = 1\n\
                fun accept(own count: Int, inout item: Resource): Unit {}\n\
                fun valid(own input: Resource): Unit {\n\
                    var local = input\n\
                    val result = accept(read(local), &local)\n\
                }";
    for rooted in [false, true] {
        let fixture = ownership_fixture(text, rooted);
        let owned = fixture.shared();
        assert_calls(
            &fixture,
            owned,
            &[
                ("read(local)", LoanKind::Shared),
                ("accept(read(local), &local)", LoanKind::Exclusive),
            ],
        );
    }
}

fn ownership_fixture(text: &str, rooted: bool) -> Fixture {
    let fixture = Fixture::ownership_source(text, rooted);
    let [entry] = fixture.names.names().index().source_units() else {
        panic!("one original source");
    };
    assert_eq!(fixture.sources.source_text(entry.source_id()), Ok(text));
    match (rooted, entry.key()) {
        (false, SourceUnitKey::MemoryDocument) => {}
        (true, SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root");
            assert_eq!(logical_path.as_str(), "ownership.ko");
        }
        _ => panic!("requested Unit layout"),
    }
    fixture
}

#[derive(Debug, PartialEq, Eq)]
enum CallLoanStep {
    Begin(UnitLoanFact),
    End(UnitLoanFact, UnitDropPoint),
}

fn assert_calls(fixture: &Fixture, owned: CompilationUnitOwnership, expected: &[(&str, LoanKind)]) {
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(owned.clone().validate().is_ok());
    let loans = owned.loans();
    assert_eq!(loans.len(), expected.len());
    let mut expected_events = Vec::new();
    for (loan, &(source_call, kind)) in loans.iter().zip(expected) {
        assert_eq!(loan.kind(), kind);
        assert!(matches!(loan.target(), UnitLoanTarget::Place(_)));
        let call = fixture.expression(0, source_call);
        assert_eq!(loan.call(), UnitExpressionId::new(fixture.source(0), call));
        assert_eq!(loan.argument().source_unit(), fixture.source(0));
        assert_eq!(
            fixture.parsed[0]
                .ast()
                .expressions()
                .get(call)
                .unwrap()
                .span(),
            loan.end_span()
        );
        assert_eq!(fixture.sources.slice(loan.end_span()), Ok(source_call));
        assert_eq!(
            loans
                .iter()
                .filter(|candidate| candidate.argument() == loan.argument())
                .count(),
            1
        );
        expected_events.extend([
            CallLoanStep::Begin(loan.clone()),
            CallLoanStep::End(loan.clone(), UnitDropPoint::CallReturn(loan.call())),
        ]);
    }

    compare_and_inspect(fixture, &fixture.parts(&owned), |checker| {
        let make = || {
            super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            )
        };
        let baseline = make().run().unwrap();
        let mut planner = make();
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        assert_eq!(baseline.facts, observed.facts);
        assert_eq!(
            baseline.conditional_receiver_facts,
            observed.conditional_receiver_facts
        );
        assert_eq!(
            baseline.direct_call_loan_ends(),
            observed.direct_call_loan_ends()
        );
        let events = observed
            .origin_probe
            .unwrap()
            .into_iter()
            .filter_map(|event| match event {
                Event::BorrowRegistered(loan) => Some(CallLoanStep::Begin(loan)),
                Event::DirectBorrowEnded { loan, point }
                | Event::UnwindBorrowRemoved { loan, point } => {
                    Some(CallLoanStep::End(loan, point))
                }
                Event::PendingBorrowRemoved { loan, call } => {
                    Some(CallLoanStep::End(loan, UnitDropPoint::CallReturn(call)))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        // Includes the explicit inout end, which is not in Unit's public Shared end projection.
        // Equality proves exact identities and order, so an inner loan cannot survive to outer inout.
        assert_eq!(events, expected_events);
    });
}
