//! Ordinary failures after a real result commit and binding retirement.
use super::*;

#[test]
fn result_action_prefix_is_revoked_from_raw_and_public_products() {
    assert_rejections_clear(
        "fun view(source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); println(item) }",
        false,
        |accepted, raw| {
            assert_eq!(raw.borrow_call_handoffs.len(), 1);
            assert_eq!(
                raw.borrow_call_handoffs,
                accepted.shared().borrow_call_handoffs()
            );
            let actions = raw.loan_action_observations[0].actions();
            assert!(actions.iter().any(|action| matches!(
                action,
                crate::ownership_checking::UnitLoanAction::BorrowHandoffCommitted(_)
            )));
            assert!(actions.iter().any(|action| matches!(
                action,
                crate::ownership_checking::UnitLoanAction::BorrowBindingEnd(_)
            )));
        },
    );
}

#[test]
fn consumer_dependency_prefix_is_revoked_from_raw_and_public_products() {
    assert_rejections_clear(
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }",
        false,
        |accepted, raw| {
            assert_eq!(raw.borrow_call_transports.len(), 1);
            assert_eq!(
                raw.borrow_call_transports,
                accepted.shared().borrow_call_transports()
            );
            assert!(
                raw.loan_action_observations[0]
                    .regions()
                    .iter()
                    .any(|region| region.protocol_version() == 4
                        && region.coverage()
                            == crate::ownership_checking::UnitLoanActionCoverage::Complete)
            );
            assert!(raw.loan_action_observations[0].actions().iter().any(|action| matches!(action, crate::ownership_checking::UnitLoanAction::BorrowConsumerRetired(retired) if retired.transfer().is_some())));
        },
    );
}
