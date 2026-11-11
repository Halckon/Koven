//! Constructed foreign and malformed handles are CI-only. Do not run locally.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{BorrowBindingEndFact, CompilationUnitOwnership};

const SOURCE: &str = r#"fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun run(): Unit { val owner = "kept"; borrow val parent = view(owner); borrow val child = parent; consume(child) }"#;

fn recorded(shared: &CompilationUnitOwnership) -> ActionSource {
    let transport = &shared.borrow_binding_transports()[0];
    let child = shared
        .borrow_results()
        .bindings()
        .iter()
        .find(|fact| fact.parent() == Some(transport.binding()))
        .unwrap()
        .clone();
    let consumer = shared
        .loans()
        .iter()
        .find(|loan| loan.call() != transport.source_loan().call())
        .unwrap()
        .clone();
    let mut actions = ActionSource::default();
    let source_begin = actions.begin(transport.source_loan().clone());
    let handoff = actions.commit_borrow_handoff(shared.borrow_call_handoffs()[0].clone());
    let parent = Parent {
        binding: transport.binding(),
        source: transport.source_loan().clone(),
        source_begin,
        handoff: Some(handoff),
    };
    let binding = child.binding();
    let commit = actions
        .commit_binding_child(Commit {
            initializer: child.initializer(),
            fact: child,
            raw_parent: parent.binding,
            parent: parent.clone(),
        })
        .unwrap();
    let begin = actions.begin(consumer.clone());
    let use_return = actions.return_binding_child_use(UseReturn {
        binding,
        parent: parent.clone(),
        commit: commit.clone(),
        loan: consumer,
        begin,
    });
    let point = shared
        .borrow_results()
        .ends()
        .iter()
        .find(|end| end.binding() == binding)
        .unwrap()
        .point();
    let binding_end = actions.end_borrow_binding(BorrowBindingEndFact { binding, point });
    actions.retire_binding_child(Retirement {
        binding,
        parent,
        commit,
        use_return,
        binding_end,
        point,
    });
    actions
}

#[test]
fn ci_only_child_links_reject_foreign_order_parent_and_initializer_faults() {
    let fixture = Fixture::new(&[("a.ko", SOURCE)]);
    let shared = fixture.shared();
    let source = shared.borrow_binding_transports()[0].source_loan();
    for fault in 0..11 {
        let mut actions = recorded(&shared);
        let handoffs = actions.borrow_handoffs();
        let ends = actions.borrow_binding_ends();
        match fault {
            0 => {}
            1..=4 => {
                let Record::BorrowBindingChildCommitted(fact) = &mut actions.records[2] else {
                    unreachable!()
                };
                match fault {
                    1 => fact.parent.source_begin.as_mut().unwrap().identity = Arc::new(()),
                    2 => fact.parent.handoff.as_mut().unwrap().index = 0,
                    3 => fact.raw_parent = fact.fact.binding(),
                    _ => fact.fact.initializer = source.call(),
                }
            }
            5..=7 => {
                let Record::BorrowBindingChildUseReturned(fact) = &mut actions.records[4] else {
                    unreachable!()
                };
                match fault {
                    5 => fact.begin.as_mut().unwrap().identity = Arc::new(()),
                    6 => fact.commit.index = 4,
                    _ => fact.parent.source_begin = None,
                }
            }
            _ => {
                let Record::BorrowBindingChildRetired(fact) = &mut actions.records[6] else {
                    unreachable!()
                };
                match fault {
                    8 => fact.binding_end.identity = Arc::new(()),
                    9 => fact.point = UnitDropPoint::CallReturn(source.call()),
                    _ => fact.commit.index = 4,
                }
            }
        }
        assert_eq!(actions.borrow_handoffs(), handoffs);
        assert_eq!(actions.borrow_binding_ends(), ends);
        let observation = actions
            .finish(source.call().source_unit(), fixture.parsed[0].source_id())
            .remove(0);
        let UnitLoanAction::BorrowBindingChildCommitted(commit) = &observation.actions()[2] else {
            unreachable!()
        };
        let UnitLoanAction::BorrowBindingChildUseReturned(used) = &observation.actions()[4] else {
            unreachable!()
        };
        let UnitLoanAction::BorrowBindingChildRetired(retired) = &observation.actions()[6] else {
            unreachable!()
        };
        assert_eq!(commit.parent_handoff().is_some(), fault == 0 || fault >= 5);
        assert_eq!(used.commit().is_some(), fault == 0 || fault >= 8);
        assert_eq!(retired.use_return().is_some(), fault == 0);
        if fault == 0 {
            assert_eq!(commit.source_begin(), observation.action_id(0));
            assert_eq!(commit.parent_handoff(), observation.action_id(1));
            assert_eq!(used.commit(), observation.action_id(2));
            assert_eq!(used.begin(), observation.action_id(3));
            assert_eq!(retired.use_return(), observation.action_id(4));
            assert_eq!(retired.binding_end(), observation.action_id(5));
        }
    }
}
