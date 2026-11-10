//! Constructed invalid links are CI-only. Do not execute locally.
use super::super::ActionSource;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{UnitBorrowCallTransportFact, UnitLoanAction, UnitLoanFact};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }";

fn recorded(transaction: &UnitBorrowCallTransportFact) -> ActionSource {
    let [consumer, source] = transaction.ended_loans();
    let mut actions = ActionSource::default();
    let handle = actions.transfer_consumer(source, consumer.call(), consumer.argument(), None);
    actions.retire_consumer(Some(handle), transaction.clone(), None);
    actions
}

fn assert_unlinked(
    actions: ActionSource,
    fixture: &Fixture,
    transaction: &UnitBorrowCallTransportFact,
) {
    assert_eq!(
        actions.consumer_transports(),
        std::slice::from_ref(transaction),
        "link validation must not delete legacy facts"
    );
    let source = transaction.ended_loans()[0].call().source_unit();
    assert!(
        validate(
            &actions.records,
            &actions.identity,
            source,
            fixture.parsed[0].source_id()
        )
        .is_empty()
    );
    let observations = actions.finish(source, fixture.parsed[0].source_id());
    let retired = observations[0]
        .actions()
        .iter()
        .find_map(|action| match action {
            UnitLoanAction::BorrowConsumerRetired(retired) => Some(retired),
            _ => None,
        })
        .unwrap();
    assert!(retired.transfer().is_none());
    assert_eq!(retired.transaction(), transaction);
}

#[test]
fn ci_only_invalid_consumer_handles_never_grant_a_dependency_id() {
    let fixture = Fixture::new(&[("a.ko", SOURCE)]);
    let transaction = fixture.shared().borrow_call_transports()[0].clone();
    let [consumer, source] = transaction.ended_loans();
    let valid = recorded(&transaction);
    assert_eq!(
        validate(
            &valid.records,
            &valid.identity,
            source.call().source_unit(),
            fixture.parsed[0].source_id()
        )
        .get(&1),
        Some(&0)
    );
    for index in [1, usize::MAX] {
        let mut actions = recorded(&transaction);
        let Record::BorrowConsumerRetired {
            transfer: Some(handle),
            ..
        } = &mut actions.records[1]
        else {
            panic!("fixture handle");
        };
        handle.index = index;
        assert_unlinked(actions, &fixture, &transaction);
    }
    let mut actions = recorded(&transaction);
    actions.records[0] = Record::Begin(source.clone());
    assert_unlinked(actions, &fixture, &transaction);
    let mut actions = recorded(&transaction);
    let mut other = ActionSource::default();
    let foreign = other.transfer_consumer(source, consumer.call(), consumer.argument(), None);
    let Record::BorrowConsumerRetired { transfer, .. } = &mut actions.records[1] else {
        panic!("fixture record");
    };
    *transfer = Some(foreign);
    assert_unlinked(actions, &fixture, &transaction);
    for wrong_argument in [false, true] {
        let mut actions = recorded(&transaction);
        let Record::BorrowConsumerTransfer {
            consumer: call,
            argument,
            ..
        } = &mut actions.records[0]
        else {
            panic!("fixture transfer");
        };
        if wrong_argument {
            *argument = source.argument();
        } else {
            *call = source.call();
        }
        assert_unlinked(actions, &fixture, &transaction);
    }
    let mut actions = recorded(&transaction);
    let Record::BorrowConsumerTransfer { source: actual, .. } = &mut actions.records[0] else {
        panic!("fixture transfer");
    };
    *actual = UnitLoanFact::new(
        source.call(),
        source.argument(),
        source.target().clone(),
        source.kind(),
        source.begin_span(),
        source.end_span(),
        None,
    );
    assert_unlinked(actions, &fixture, &transaction);
    for kind in [LoanKind::Exclusive, LoanKind::Shared] {
        let target = if kind == LoanKind::Exclusive {
            source.target().clone()
        } else {
            UnitLoanTarget::Temporary(source.argument())
        };
        let malformed = UnitBorrowCallTransportFact {
            source: UnitLoanFact::new(
                source.call(),
                source.argument(),
                target.clone(),
                kind,
                source.begin_span(),
                source.end_span(),
                source.parameter_span(),
            ),
            consumer: UnitLoanFact::new(
                consumer.call(),
                consumer.argument(),
                target,
                kind,
                consumer.begin_span(),
                consumer.end_span(),
                consumer.parameter_span(),
            ),
        };
        assert_unlinked(recorded(&malformed), &fixture, &malformed);
    }
    let foreign = Fixture::new(&[("a.ko", SOURCE)]);
    let foreign_transaction = foreign.shared().borrow_call_transports()[0].clone();
    assert_eq!(
        foreign_transaction.ended_loans()[0].call().source_unit(),
        consumer.call().source_unit()
    );
    assert_ne!(
        foreign_transaction.ended_loans()[0]
            .begin_span()
            .source_id(),
        consumer.begin_span().source_id()
    );
    assert_unlinked(
        recorded(&foreign_transaction),
        &fixture,
        &foreign_transaction,
    );
}

#[test]
fn ci_only_invalid_acquisitions_preserve_existing_transport_link() {
    let fixture = Fixture::new(&[("a.ko", SOURCE)]);
    let transaction = fixture.shared().borrow_call_transports()[0].clone();
    let [consumer, source] = transaction.ended_loans();
    for fault in 0..10 {
        let mut actions = ActionSource::default();
        let source_handle = actions.begin(source.clone());
        let transfer =
            actions.transfer_consumer(source, consumer.call(), consumer.argument(), source_handle);
        let consumer_handle = actions.begin(consumer.clone());
        actions.retire_consumer(Some(transfer), transaction.clone(), consumer_handle);
        match fault {
            0 => {}
            1..=4 => {
                let Record::BorrowConsumerTransfer { acquisition, .. } = &mut actions.records[1]
                else {
                    unreachable!()
                };
                match fault {
                    1 => *acquisition = None,
                    2 => acquisition.as_mut().unwrap().index = usize::MAX,
                    3 => acquisition.as_mut().unwrap().index = 1,
                    _ => *acquisition = ActionSource::default().begin(source.clone()),
                }
            }
            5..=7 => {
                let Record::BorrowConsumerRetired { acquisition, .. } = &mut actions.records[3]
                else {
                    unreachable!()
                };
                match fault {
                    5 => *acquisition = None,
                    6 => acquisition.as_mut().unwrap().index = 0,
                    _ => *acquisition = ActionSource::default().begin(consumer.clone()),
                }
            }
            8 => actions.records[0] = Record::Begin(consumer.clone()),
            _ => actions.records[2] = Record::Begin(source.clone()),
        }
        assert_eq!(
            actions.consumer_transports(),
            std::slice::from_ref(&transaction)
        );
        let observation = actions
            .finish(source.call().source_unit(), fixture.parsed[0].source_id())
            .remove(0);
        let UnitLoanAction::BorrowConsumerRetired(retired) = &observation.actions()[3] else {
            unreachable!()
        };
        assert!(
            retired.transfer().is_some(),
            "old real relation remains independently valid"
        );
        assert_eq!(retired.transaction(), &transaction);
        assert_eq!(retired.source_begin().is_some(), fault == 0);
        assert_eq!(retired.consumer_begin().is_some(), fault == 0);
    }
}
