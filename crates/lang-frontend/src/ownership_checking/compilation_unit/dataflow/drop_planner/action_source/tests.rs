//! Actual cross-source results plus a CI-only synthetic projection guard.
use super::*;
use crate::ownership_checking::UnitLoanAction;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

const VIEW: &str = "fun view(source: String): borrow String from source = source";
const CALLER: &str =
    "fun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); println(item) }";

#[test]
fn result_action_cross_source_preserves_caller_and_callee_lineage() {
    for sources in [
        [("a.ko", VIEW), ("b.ko", CALLER)],
        [("b.ko", CALLER), ("a.ko", VIEW)],
    ] {
        let fixture = Fixture::new(&sources);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let observation = shared
            .loan_action_observations()
            .iter()
            .find(|observation| {
                observation
                    .actions()
                    .iter()
                    .any(|action| matches!(action, UnitLoanAction::BorrowHandoffCommitted(_)))
            })
            .unwrap();
        let other = shared
            .loan_action_observations()
            .iter()
            .find(|entry| entry.source() != observation.source())
            .unwrap();
        let mut count = 0;
        for (index, action) in observation.actions().iter().enumerate() {
            match action {
                UnitLoanAction::BorrowHandoffCommitted(handoff) => {
                    assert_eq!(handoff.binding().source_unit(), observation.source());
                    assert_eq!(
                        handoff.source_loan().call().source_unit(),
                        observation.source()
                    );
                    assert_ne!(
                        handoff.source_loan().begin_span().source_id(),
                        handoff.source_loan().parameter_span().unwrap().source_id()
                    );
                }
                UnitLoanAction::BorrowBindingEnd(end) => {
                    assert_eq!(end.binding().source_unit(), observation.source());
                }
                _ => continue,
            }
            count += 1;
            let id = observation.action_id(index).unwrap();
            assert!(other.action(&id).is_none());
        }
        assert_eq!(count, 2);
    }
}

// Explicitly constructed duplicate records are CI-only. Local selections must not run this.
#[test]
fn ci_only_duplicate_binding_records_preserve_identity_and_legacy_deduplication() {
    let fixture = Fixture::new(&[("a.ko", VIEW), ("b.ko", CALLER)]);
    let shared = fixture.shared();
    let end = shared.borrow_results().ends()[0];
    for enabled in [true, false] {
        let mut source = ActionSource {
            enabled,
            ..ActionSource::default()
        };
        source.end_borrow_binding(end);
        source.end_borrow_binding(end);
        assert_eq!(source.borrow_binding_ends(), [end]);
        assert_eq!(source.len(), 0);
        assert_eq!(source.iter().count(), 0);
        let observations =
            source.finish(end.binding().source_unit(), fixture.parsed[1].source_id());
        if enabled {
            let observation = &observations[0];
            assert_eq!(observation.actions().len(), 2);
            assert_ne!(observation.action_id(0), observation.action_id(1));
        } else {
            assert!(observations.is_empty());
        }
    }
}

#[test]
fn consumer_dependency_cross_source_keeps_external_parameter_spans() {
    let fixture = Fixture::new(&[
        (
            "a.ko",
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}",
        ),
        (
            "b.ko",
            "fun run(): Unit { val owner = \"kept\"; consume(view(owner)) }",
        ),
    ]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    let observation = shared
        .loan_action_observations()
        .iter()
        .find(|entry| {
            entry
                .actions()
                .iter()
                .any(|action| matches!(action, UnitLoanAction::BorrowConsumerRetired(_)))
        })
        .unwrap();
    let other = shared
        .loan_action_observations()
        .iter()
        .find(|entry| entry.source() != observation.source())
        .unwrap();
    for action in observation.actions() {
        if let UnitLoanAction::BorrowConsumerRetired(retired) = action {
            let handle = retired.transfer().unwrap();
            assert!(observation.action(&handle).is_some());
            assert!(other.action(&handle).is_none());
            for (id, loan) in [
                (
                    retired.consumer_begin().unwrap(),
                    &retired.transaction().ended_loans()[0],
                ),
                (
                    retired.source_begin().unwrap(),
                    &retired.transaction().ended_loans()[1],
                ),
            ] {
                assert_eq!(
                    observation.action(&id),
                    Some(&UnitLoanAction::Begin((*loan).clone()))
                );
                assert!(other.action(&id).is_none());
            }

            for loan in retired.transaction().ended_loans() {
                assert_eq!(loan.call().source_unit(), observation.source());
                assert_eq!(loan.begin_span().source_id(), fixture.parsed[1].source_id());
                assert_ne!(
                    loan.parameter_span().unwrap().source_id(),
                    loan.begin_span().source_id()
                );
            }
        }
    }
}
