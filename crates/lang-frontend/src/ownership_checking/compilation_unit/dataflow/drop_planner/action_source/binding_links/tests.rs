//! Constructed invalid links are CI-only. Do not execute locally.
use super::super::ActionSource;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{BorrowBindingEndFact, CompilationUnitOwnership, UnitLoanAction};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }";

fn recorded(shared: &CompilationUnitOwnership) -> ActionSource {
    let transaction = shared.borrow_binding_transports()[0].clone();
    let source = transaction.source_loan();
    let consumer = shared
        .loans()
        .iter()
        .find(|loan| loan.call() != source.call())
        .unwrap();
    let mut actions = ActionSource::default();
    let source_begin = actions.begin(source.clone());
    let handoff = actions.commit_borrow_handoff(shared.borrow_call_handoffs()[0].clone());
    let begin = actions.begin(consumer.clone());
    let use_return = actions.return_binding_use(
        transaction.binding(),
        consumer.clone(),
        source_begin.clone(),
        Some(handoff.clone()),
        begin,
    );
    let binding_end = actions.end_borrow_binding(BorrowBindingEndFact {
        binding: transaction.binding(),
        point: transaction.point(),
    });
    actions.retire_binding_source(
        transaction,
        source_begin,
        Some(handoff),
        use_return,
        binding_end,
    );
    actions
}

#[test]
fn ci_only_binding_links_reject_foreign_reversed_and_wrong_role_handles() {
    let fixture = Fixture::new(&[("a.ko", SOURCE)]);
    let shared = fixture.shared();
    let transaction = &shared.borrow_binding_transports()[0];
    let source = transaction.source_loan().call().source_unit();
    for fault in 0..9 {
        let mut actions = recorded(&shared);
        let expected_handoffs = actions.borrow_handoffs();
        let expected_ends = actions.borrow_binding_ends();
        match fault {
            0 => {}
            1..=4 => {
                let Record::BorrowBindingUseReturned(fact) = &mut actions.records[3] else {
                    unreachable!()
                };
                match fault {
                    1 => fact.source_begin.as_mut().unwrap().identity = Arc::new(()),
                    2 => fact.handoff.as_mut().unwrap().index = 2,
                    3 => fact.begin.as_mut().unwrap().index = 3,
                    _ => fact.begin.as_mut().unwrap().index = 0,
                }
            }
            5..=7 => {
                let Record::BorrowBindingSourceRetired(fact) = &mut actions.records[5] else {
                    unreachable!()
                };
                match fault {
                    5 => fact.binding_end.identity = Arc::new(()),
                    6 => fact.binding_end.index = 3,
                    _ => fact.handoff.as_mut().unwrap().index = usize::MAX,
                }
            }
            _ => {
                let Record::BorrowBindingSourceRetired(fact) = &mut actions.records[5] else {
                    unreachable!()
                };
                fact.use_return.as_mut().unwrap().identity = Arc::new(());
            }
        }
        assert_eq!(actions.borrow_handoffs(), expected_handoffs);
        assert_eq!(actions.borrow_binding_ends(), expected_ends);
        let observation = actions
            .finish(source, fixture.parsed[0].source_id())
            .remove(0);
        let UnitLoanAction::BorrowBindingUseReturned(returned) = &observation.actions()[3] else {
            unreachable!()
        };
        let UnitLoanAction::BorrowBindingSourceRetired(retired) = &observation.actions()[5] else {
            unreachable!()
        };
        assert_eq!(returned.begin().is_some(), fault == 0 || fault >= 5);
        assert_eq!(retired.use_return().is_some(), fault == 0);
        assert_eq!(retired.transaction(), transaction);
        if fault == 0 {
            assert_eq!(returned.source_begin(), observation.action_id(0));
            assert_eq!(returned.handoff(), observation.action_id(1));
            assert_eq!(returned.begin(), observation.action_id(2));
            assert_eq!(retired.use_return(), observation.action_id(3));
            assert_eq!(retired.binding_end(), observation.action_id(4));
        }
    }
}
