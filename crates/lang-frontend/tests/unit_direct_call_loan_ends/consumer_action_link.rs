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

fn assert_links(observed: &Observed, count: usize) {
    assert_units_admitted(observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let actions = observation.actions();
        let retired: Vec<_> = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                UnitLoanAction::BorrowConsumerRetired(retirement) => Some((index, retirement)),
                _ => None,
            })
            .collect();
        assert_eq!(retired.len(), count);
        assert_eq!(unit.borrow_call_transports().len(), count);
        let mut handles = Vec::new();
        for ((retire_index, retirement), old) in retired.iter().zip(unit.borrow_call_transports()) {
            assert_eq!(retirement.transaction(), old);
            let handle = retirement.transfer().expect("proved actual edge");
            let transfer_index = (0..actions.len())
                .find(|&index| observation.action_id(index).as_ref() == Some(&handle))
                .unwrap();
            let Some(UnitLoanAction::BorrowConsumerTransfer {
                source,
                consumer,
                argument,
            }) = observation.action(&handle)
            else {
                panic!("typed dependency");
            };
            let [consumer_loan, source_loan] = old.ended_loans();
            assert_eq!(source, source_loan);
            assert_eq!(*consumer, consumer_loan.call());
            assert_eq!(*argument, consumer_loan.argument());
            let source_begin = actions.iter().position(|action| matches!(action, UnitLoanAction::Begin(actual) if actual == source_loan)).unwrap();
            let consumer_begin = actions.iter().position(|action| matches!(action, UnitLoanAction::Begin(actual) if actual == consumer_loan)).unwrap();
            assert!(
                source_begin < transfer_index
                    && transfer_index < consumer_begin
                    && consumer_begin < *retire_index
            );
            assert!(!handles.contains(&handle));
            handles.push(handle);
        }
        assert_ne!(observation.coverage(), UnitLoanActionCoverage::Complete);
    }
}

#[test]
fn grouped_and_named_sibling_consumers_keep_actual_distinct_links() {
    let grouped = observe(
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(((view((owner))))) }",
    );
    assert_links(&grouped, 1);
    let siblings = observe(
        "fun view(source: String): borrow String from source = source\nfun consume(first: String, second: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(second = view(owner), first = view(owner)) }",
    );
    assert_links(&siblings, 2);
    for unit in &siblings.units {
        let observation = &unit.loan_action_observations()[0];
        let retired: Vec<_> = observation
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowConsumerRetired(retired) => Some(retired),
                _ => None,
            })
            .collect();
        let positions: Vec<_> = retired
            .iter()
            .map(|retired| {
                (0..observation.actions().len())
                    .find(|&index| observation.action_id(index) == retired.transfer())
                    .unwrap()
            })
            .collect();
        assert!(
            positions[0] > positions[1],
            "actual reverse-operand retirement, not source-span sorting"
        );
    }
}

#[test]
fn later_return_keeps_existing_rejection_and_publishes_no_dependency() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun consume(item: String, own next: Int): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner), return) }",
    );
    for unit in &observed.units {
        let [diagnostic] = unit.diagnostics() else {
            panic!("existing unsupported continuation");
        };
        assert_eq!(diagnostic.code().to_string(), "L0164");
        assert_eq!(
            diagnostic.message(),
            "caller borrow result continuation is not yet proven"
        );
        let span = diagnostic.primary_span();
        assert_eq!(
            (span.source_id(), span.start(), span.end()),
            (observed.parsed.source_id(), 157, 168)
        );
        assert!(unit.borrow_call_transports().is_empty());
        assert!(unit.loan_action_observations().is_empty());
    }
}

#[test]
fn consumer_link_survives_snapshot_clone_but_not_reanalysis() {
    let left = observe(SOURCE);
    let right = observe(SOURCE);
    assert_links(&left, 1);
    assert_links(&right, 1);
    for (left, right) in left.units.iter().zip(&right.units) {
        let left = &left.loan_action_observations()[0];
        let right = &right.loan_action_observations()[0];
        let cloned = left.clone();
        assert_eq!(left, &cloned);
        let handle = left
            .actions()
            .iter()
            .find_map(|action| match action {
                UnitLoanAction::BorrowConsumerRetired(retired) => retired.transfer(),
                _ => None,
            })
            .unwrap();
        assert!(cloned.action(&handle).is_some());
        assert!(right.action(&handle).is_none());
    }
}

#[test]
fn nonowning_parameter_source_does_not_gain_transport_coverage() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(source: String): Unit { consume(view(source)) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(unit.borrow_call_transports().is_empty());
        let observation = &unit.loan_action_observations()[0];
        assert!(
            !observation
                .actions()
                .iter()
                .any(|action| matches!(action, UnitLoanAction::BorrowConsumerRetired(_)))
        );
        assert_ne!(observation.coverage(), UnitLoanActionCoverage::Complete);
    }
}
