//! Actual synchronous transport links; no additional Complete family.
use super::*;
use lang_frontend::ownership_checking::{UnitLoanAction, UnitLoanActionCoverage};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }";

#[test]
fn consumer_retirement_references_its_actual_transfer_record() {
    let observed = observe(SOURCE);
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [transaction] = unit.borrow_call_transports() else {
            panic!("old actual transport");
        };
        let observation = &unit.loan_action_observations()[0];
        let retirement = observation
            .actions()
            .iter()
            .find_map(|action| match action {
                UnitLoanAction::BorrowConsumerRetired(retirement) => Some(retirement),
                _ => None,
            });
        assert!(
            retirement.is_some(),
            "actual consumer/source retirement is missing from the unified source"
        );
        let retirement = retirement.unwrap();
        assert_eq!(retirement.transaction(), transaction);
        let id = retirement
            .transfer()
            .expect("validated actual transfer dependency");
        let Some(UnitLoanAction::BorrowConsumerTransfer {
            source,
            consumer,
            argument,
        }) = observation.action(&id)
        else {
            panic!("same catalog transfer record");
        };
        let [ended_consumer, ended_source] = transaction.ended_loans();
        assert_eq!(source, ended_source);
        assert_eq!(*consumer, ended_consumer.call());
        assert_eq!(*argument, ended_consumer.argument());
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
    }
}
