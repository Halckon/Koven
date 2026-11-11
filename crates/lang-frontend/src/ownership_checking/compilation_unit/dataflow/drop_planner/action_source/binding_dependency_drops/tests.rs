//! Constructed malformed evidence is CI-only. Compile locally; do not execute locally.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{
    BorrowBindingEndFact, UnitDropPoint, UnitOwnerCallable, UnitOwnerId,
};
use crate::type_checking::UnitItemId;

const SOURCE: &str = r#"fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun run(): Unit { val owner = "kept"; borrow val parent = view(owner); borrow val child = parent; consume(child) }"#;

fn recorded(fixture: &Fixture) -> (ActionSource, ActionHandle, ActionHandle, OwnedValue) {
    let shared = fixture.shared();
    let tx = &shared.borrow_binding_transports()[0];
    let child = shared
        .borrow_results()
        .bindings()
        .iter()
        .find(|b| b.parent() == Some(tx.binding()))
        .unwrap()
        .clone();
    let loan = shared
        .loans()
        .iter()
        .find(|l| l.call() != tx.source_loan().call())
        .unwrap()
        .clone();
    let mut actions = ActionSource::default();
    let source_begin = actions.begin(tx.source_loan().clone());
    let handoff = actions.commit_borrow_handoff(shared.borrow_call_handoffs()[0].clone());
    let parent = binding_child_links::Parent {
        binding: tx.binding(),
        source: tx.source_loan().clone(),
        source_begin: source_begin.clone(),
        handoff: Some(handoff.clone()),
    };
    let binding = child.binding();
    let commit = actions
        .commit_binding_child(binding_child_links::Commit {
            initializer: child.initializer(),
            raw_parent: parent.binding,
            fact: child,
            parent: parent.clone(),
        })
        .unwrap();
    let begin = actions.begin(loan.clone());
    let use_return = actions.return_binding_child_use(binding_child_links::UseReturn {
        binding,
        parent: parent.clone(),
        commit: commit.clone(),
        loan,
        begin,
    });
    let child_end = actions.end_borrow_binding(BorrowBindingEndFact {
        binding,
        point: tx.point(),
    });
    let retired = actions
        .retire_binding_child(binding_child_links::Retirement {
            binding,
            parent,
            commit,
            use_return,
            binding_end: child_end,
            point: tx.point(),
        })
        .unwrap();
    let parent_end = actions.end_borrow_binding(BorrowBindingEndFact {
        binding: tx.binding(),
        point: tx.point(),
    });
    let parent_retired = actions
        .retire_binding_source(tx.clone(), source_begin, Some(handoff), None, parent_end)
        .unwrap();
    let drop = shared
        .drops()
        .iter()
        .find(|drop| matches!(drop.target(), UnitDropTarget::Named(_)))
        .unwrap();
    let UnitDropTarget::Named(root) = drop.target() else {
        unreachable!()
    };
    let owner = OwnedValue {
        owner: Some(UnitOwnerId {
            identity: Arc::new(()),
            callable: UnitOwnerCallable::Function(UnitItemId::new(
                root.source_unit(),
                fixture.parsed[0].roots()[2],
            )),
            index: 0,
        }),
        symbol: root,
        origin: drop.value_origin(),
        declaration: drop.value_origin(),
        scope_depth: 1,
    };
    actions.attach_binding_dependency(&parent_retired, retired.clone(), owner.clone());
    (actions, retired, parent_retired, owner)
}

#[test]
fn ci_only_dependency_drop_requires_carried_catalog_order_owner_and_new_append() {
    let fixture = Fixture::new(&[("a.ko", SOURCE)]);
    for fault in 0..12 {
        let (mut actions, child, parent, owner) = recorded(&fixture);
        let source = owner.symbol.source_unit();
        if fault == 11 {
            actions.attach_binding_dependency(&parent, child.clone(), owner.clone());
            actions.attach_binding_dependency(&parent, child.clone(), owner.clone());
            assert!(actions.dependency_drop_cause(child, parent).is_none());
            continue;
        }
        let mut cause = actions
            .dependency_drop_cause(child.clone(), parent.clone())
            .unwrap();
        if fault == 1 {
            let BindingDropCause::Dependency(cause) = &mut cause else {
                unreachable!()
            };
            cause.child.identity = Arc::new(());
        }
        if fault == 2 {
            let BindingDropCause::Dependency(cause) = &mut cause else {
                unreachable!()
            };
            cause.parent.index = child.index;
        }
        let mut removed = owner.clone();
        if fault == 3 {
            removed.owner.as_mut().unwrap().identity = Arc::new(());
        }
        if fault == 4 {
            removed.origin = fixture.parsed[0]
                .ast()
                .items()
                .get(fixture.parsed[0].roots()[0])
                .unwrap()
                .span();
        }
        let checked = actions.verify_removed_binding_owner(cause, &removed);
        let tx_point = match &actions.records[8] {
            Record::BorrowBindingSourceRetired(fact) => fact.transaction.point(),
            _ => unreachable!(),
        };
        let UnitDropPoint::AfterStatement(statement) = tx_point else {
            unreachable!()
        };
        if fault == 5 {
            let Record::BorrowBindingChildRetired(fact) = &mut actions.records[6] else {
                unreachable!()
            };
            fact.use_return = None;
        }
        if fault == 6 {
            let Record::BorrowBindingSourceRetired(fact) = &mut actions.records[8] else {
                unreachable!()
            };
            fact.dependency.as_mut().unwrap().child.index = 4;
        }
        if fault == 7 {
            actions.end_borrow_binding(BorrowBindingEndFact {
                binding: owner.symbol,
                point: tx_point,
            });
        }
        if fault != 8 {
            // Fault 8 models the real planner's existing fact-dedup/no-new-append gate.
            let drop = crate::ownership_checking::UnitDropFact::new(
                tx_point,
                UnitDropTarget::Named(owner.symbol),
                owner.origin,
            );
            actions.push_with_binding_drop(
                (
                    super::super::PlannerDropPoint::AfterStatement(statement.statement()),
                    UnitIterationCleanupAction::Drop(drop),
                ),
                checked,
            );
        }
        if fault == 9 {
            let mut duplicate = actions.dependency_drop_cause(child, parent).unwrap();
            duplicate = actions
                .verify_removed_binding_owner(duplicate, &owner)
                .unwrap();
            let cleanup = ActionHandle {
                identity: actions.identity.clone(),
                index: 9,
            };
            actions.record_binding_drop(duplicate, cleanup);
        }
        if fault == 10 {
            let Record::BorrowBindingSourceRetired(fact) = &mut actions.records[8] else {
                unreachable!()
            };
            fact.dependency
                .as_mut()
                .unwrap()
                .owner
                .owner
                .as_mut()
                .unwrap()
                .identity = Arc::new(());
        }
        let result = actions.finish(source, fixture.parsed[0].source_id());
        assert_eq!(
            result[0].binding_dependency_drops().len(),
            usize::from(fault == 0)
        );
    }
}
