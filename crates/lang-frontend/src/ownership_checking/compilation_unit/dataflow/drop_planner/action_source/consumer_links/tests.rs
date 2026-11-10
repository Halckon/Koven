//! Constructed invalid links are CI-only. Do not execute locally.
use super::super::ActionSource;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{UnitBorrowCallTransportFact, UnitLoanAction, UnitLoanFact};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }";

fn recorded(transaction: &UnitBorrowCallTransportFact) -> ActionSource {
    let [consumer, source] = transaction.ended_loans();
    let mut actions = ActionSource::default();
    let handle = actions.transfer_consumer(source, consumer.call(), consumer.argument());
    actions.retire_consumer(Some(handle), transaction.clone());
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
    let foreign = other.transfer_consumer(source, consumer.call(), consumer.argument());
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
