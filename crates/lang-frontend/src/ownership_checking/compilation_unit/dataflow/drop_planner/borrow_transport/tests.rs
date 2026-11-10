//! Synthetic branch-state guards are CI-only, never local ordinary selections.
use super::super::{action_source::ActionSource, model::merge_value_states};
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

#[test]
fn ci_only_consumer_handle_merge_preserves_semantics_and_degrades_ambiguity() {
    let fixture = Fixture::new(&[(
        "a.ko",
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }",
    )]);
    let transaction = fixture.shared().borrow_call_transports()[0].clone();
    let [consumer, source] = transaction.ended_loans();
    let mut actions = ActionSource::default();
    let first = actions.transfer_consumer(source, consumer.call(), consumer.argument(), None);
    let second = actions.transfer_consumer(source, consumer.call(), consumer.argument(), None);
    let pending = |transfer| PendingTransport {
        consumer: consumer.call(),
        argument: consumer.argument(),
        source: source.clone(),
        transfer,
    };
    assert_eq!(
        pending(Some(first.clone())),
        pending(Some(second.clone())),
        "observer metadata is not semantic equality"
    );
    let state = |transfer| ValueState {
        borrow_call_transports: vec![pending(transfer)],
        ..ValueState::default()
    };
    let cloned = merge_value_states(vec![state(Some(first.clone())), state(Some(first.clone()))]);
    assert_eq!(cloned.borrow_call_transports.len(), 1);
    assert!(
        cloned.borrow_call_transports[0]
            .transfer
            .as_ref()
            .unwrap()
            .same_record(&first)
    );
    for alternative in [Some(second), None] {
        let mut merged = merge_value_states(vec![state(Some(first.clone())), state(alternative)]);
        assert_eq!(merged.borrow_call_transports.len(), 1);
        let merged = merged.borrow_call_transports.pop().unwrap();
        assert!(merged.transfer.is_none());
        actions.retire_consumer(merged.transfer, transaction.clone(), None);
    }
    assert_eq!(
        actions.consumer_transports(),
        [transaction.clone(), transaction.clone()]
    );
    let observations = actions.finish(source.call().source_unit(), fixture.parsed[0].source_id());
    assert!(
        observations[0]
            .actions()
            .iter()
            .filter_map(|action| match action {
                crate::ownership_checking::UnitLoanAction::BorrowConsumerRetired(retired) =>
                    Some(retired),
                _ => None,
            })
            .all(|retired| retired.transfer().is_none() && retired.transaction() == &transaction)
    );
}

#[test]
fn ci_only_pending_acquisition_merge_only_degrades_observation() {
    use super::super::model::PendingLoan;
    let fixture = Fixture::new(&[(
        "a.ko",
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }",
    )]);
    let transaction = fixture.shared().borrow_call_transports()[0].clone();
    let source = transaction.ended_loans()[1];
    let mut actions = ActionSource::default();
    let first = actions.begin(source.clone()).unwrap();
    let different = actions.begin(source.clone()).unwrap();
    let foreign = ActionSource::default().begin(source.clone()).unwrap();
    let pending = |handle| PendingLoan(Action::EndCallLoan(source.clone()), 0, handle);
    let state = |pending_loans| ValueState {
        pending_loans,
        ..ValueState::default()
    };
    let merged = merge_value_states(vec![
        state(vec![pending(Some(first.clone()))]),
        state(vec![pending(Some(first.clone()))]),
    ]);
    assert!(
        merged.pending_loans[0]
            .2
            .as_ref()
            .unwrap()
            .same_record(&first)
    );
    for alternatives in [
        vec![pending(Some(different))],
        vec![pending(Some(foreign))],
        vec![pending(None)],
        vec![],
        vec![PendingLoan(
            Action::EndCallLoan(source.clone()),
            1,
            Some(first.clone()),
        )],
        vec![pending(Some(first.clone())), pending(Some(first.clone()))],
    ] {
        let merged = merge_value_states(vec![
            state(alternatives),
            state(vec![pending(Some(first.clone()))]),
            state(vec![pending(Some(first.clone()))]),
        ]);
        assert_eq!(merged.pending_loans, vec![pending(None)]);
        assert!(
            merged.pending_loans[0].2.is_none(),
            "missing or ambiguous evidence cannot be restored by later matching paths"
        );
    }
}
