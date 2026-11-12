//! Forged identity and malformed link cases are CI-only; never execute locally.
use super::super::super::{
    PlannerDropPoint,
    borrow_forward_transport::{ForwardCause, ForwardScope},
};
use super::super::ActionSource;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{
    UnitBorrowForwardTransportFact, UnitIterationCleanupAction, UnitLoanAction, UnitLoanFact,
};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)";

fn recorded(fact: &UnitBorrowForwardTransportFact, scope: ForwardScope) -> ActionSource {
    let mut actions = ActionSource::default();
    let acquisition = actions.begin(fact.source_loan().clone()).unwrap();
    actions.forward_handoff(
        fact.clone(),
        Some(ForwardCause { acquisition, scope }),
        Some(scope),
    );
    actions
}

#[test]
fn ci_only_invalid_forward_identity_never_grants_either_causal_getter() {
    let fixture = Fixture::new(&[("a.ko", SOURCE), ("b.ko", "fun other(): Unit {}")]);
    let owned = fixture.shared();
    let fact = &owned.borrow_forward_transports()[0];
    let loan = fact.source_loan();
    let declarations = fixture.names.names().index().declarations();
    let wrap = declarations.iter().find(|d| d.name() == "wrap").unwrap();
    let other = declarations.iter().find(|d| d.name() == "other").unwrap();
    let UnitLoanTarget::Place(place) = loan.target() else {
        panic!("parameter root")
    };
    let scope = ForwardScope {
        region: None,
        callable: UnitItemId::new(wrap.source_unit(), wrap.root()),
        body: loan.call(),
        control: loan.call(),
        branch: None,
        branch_entry: None,
        argument: loan.argument(),
        root: place.root(),
    };
    let source = loan.call().source_unit();
    let file = fixture.parsed[0].source_id();
    let valid = recorded(fact, scope);
    assert_eq!(
        validate(
            &valid.records,
            &valid.identity,
            source,
            file,
            &valid.branch_entries
        )
        .get(&1),
        Some(&(0, scope.callable))
    );
    let original = valid.finish(source, file);
    let UnitLoanAction::BorrowForwardHandoff(handoff) = &original[0].actions()[1] else {
        panic!("real record")
    };
    let id = handoff.source_begin().unwrap();
    assert_eq!(original[0].action(&id), original[0].clone().action(&id));
    let other_catalog = recorded(fact, scope).finish(source, file);
    assert_eq!(
        original, other_catalog,
        "structural snapshot equality is separate from identity"
    );
    assert!(other_catalog[0].action(&id).is_none());
    for fault in 0..16 {
        let mut actions = recorded(fact, scope);
        let mut checked_file = file;
        match fault {
            0 => {
                let Record::BorrowForwardHandoff { cause, .. } = &mut actions.records[1] else {
                    unreachable!()
                };
                *cause = None;
            }
            1 => {
                let Record::BorrowForwardHandoff { completed, .. } = &mut actions.records[1] else {
                    unreachable!()
                };
                *completed = None;
            }
            2..=4 => {
                let Record::BorrowForwardHandoff {
                    cause: Some(cause), ..
                } = &mut actions.records[1]
                else {
                    unreachable!()
                };
                match fault {
                    2 => cause.acquisition = ActionSource::default().begin(loan.clone()).unwrap(),
                    3 => cause.acquisition.index = 1,
                    _ => cause.acquisition.index = usize::MAX,
                }
            }
            5 => {
                actions.records[0] = Record::Cleanup((
                    PlannerDropPoint::ControlTransfer(loan.call().expression()),
                    UnitIterationCleanupAction::EndCallLoan(loan.clone()),
                ))
            }
            6 => {
                actions.records[0] = Record::Begin(UnitLoanFact::new(
                    loan.call(),
                    loan.argument(),
                    loan.target().clone(),
                    loan.kind(),
                    loan.begin_span(),
                    loan.end_span(),
                    None,
                ))
            }
            7..=9 => {
                let Record::BorrowForwardHandoff {
                    cause: Some(cause),
                    completed: Some(completed),
                    ..
                } = &mut actions.records[1]
                else {
                    unreachable!()
                };
                match fault {
                    7 => cause.scope.body = scope.argument,
                    8 => {
                        completed.callable =
                            UnitItemId::new(wrap.source_unit(), fixture.parsed[0].roots()[0])
                    }
                    _ => cause.scope.argument = scope.body,
                }
            }
            10..=11 => {
                let malformed = UnitLoanFact::new(
                    loan.call(),
                    loan.argument(),
                    if fault == 11 {
                        UnitLoanTarget::Temporary(loan.argument())
                    } else {
                        loan.target().clone()
                    },
                    if fault == 10 {
                        LoanKind::Exclusive
                    } else {
                        loan.kind()
                    },
                    loan.begin_span(),
                    loan.end_span(),
                    loan.parameter_span(),
                );
                actions.records[0] = Record::Begin(malformed.clone());
                let Record::BorrowForwardHandoff { transaction, .. } = &mut actions.records[1]
                else {
                    unreachable!()
                };
                transaction.source = malformed;
            }
            12..=13 => {
                let Record::BorrowForwardHandoff { transaction, .. } = &mut actions.records[1]
                else {
                    unreachable!()
                };
                if fault == 12 {
                    transaction.returned = scope.argument;
                } else {
                    transaction.point = UnitDropPoint::CallReturn(scope.body);
                }
            }
            14 => checked_file = fixture.parsed[1].source_id(),
            _ => {
                let Record::BorrowForwardHandoff {
                    cause: Some(cause),
                    completed: Some(completed),
                    ..
                } = &mut actions.records[1]
                else {
                    unreachable!()
                };
                let foreign = UnitItemId::new(other.source_unit(), other.root());
                cause.scope.callable = foreign;
                completed.callable = foreign;
            }
        }
        let expected = actions.forward_transports();
        assert_eq!(
            expected.len(),
            1,
            "raw transaction retained under fault {fault}"
        );
        assert!(
            validate(
                &actions.records,
                &actions.identity,
                source,
                checked_file,
                &actions.branch_entries
            )
            .is_empty(),
            "fault {fault}"
        );
        let observed = actions.finish(source, checked_file);
        let handoff = observed[0]
            .actions()
            .iter()
            .find_map(|a| {
                if let UnitLoanAction::BorrowForwardHandoff(h) = a {
                    Some(h)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(
            handoff.source_begin().is_none() && handoff.callable().is_none(),
            "fault {fault}"
        );
        assert_eq!(handoff.transaction(), &expected[0]);
    }
}
