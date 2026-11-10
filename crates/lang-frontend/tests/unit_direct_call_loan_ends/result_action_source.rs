//! Ordinary source/result transitions: Partial evidence, not another Complete protocol.
use super::*;
use lang_frontend::ownership_checking::{
    UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage,
};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); println(item) }";

#[test]
fn committed_result_and_binding_end_share_the_actual_action_source() {
    let observed = observe(SOURCE);
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("real existing handoff");
        };
        let [end] = unit.borrow_results().ends() else {
            panic!("real existing binding end");
        };
        assert_eq!(handoff.binding(), end.binding());
        let UnitLoanTarget::Place(place) = handoff.source_loan().target() else {
            panic!("real owned source");
        };
        assert!(unit.drops().iter().any(|drop| drop.point() == end.point()
            && drop.target() == UnitDropTarget::Named(place.root())));
        let observation = &unit.loan_action_observations()[0];
        let actions = observation.actions();
        assert!(actions.iter().any(|action| matches!(action, UnitLoanAction::BorrowHandoffCommitted(actual) if actual == handoff)), "actual committed handoff is missing from the unified source");
        let begin = actions.iter().position(|action| matches!(action, UnitLoanAction::Begin(loan) if loan == handoff.source_loan())).unwrap();
        let commit = actions.iter().position(|action| matches!(action, UnitLoanAction::BorrowHandoffCommitted(actual) if actual == handoff)).unwrap();
        let end_index = actions.iter().position(|action| matches!(action, UnitLoanAction::BorrowBindingEnd(actual) if actual == end)).unwrap();
        let drop_index = actions.iter().position(|action| matches!(action, UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::Drop(drop), .. } if drop.point() == end.point() && drop.target() == UnitDropTarget::Named(place.root()))).unwrap();
        assert!(begin < commit && commit < end_index && end_index < drop_index);
        assert!(!actions.iter().any(|action| matches!(action, UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::EndCallLoan(loan), .. } if loan == handoff.source_loan())));
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            observation
                .regions()
                .iter()
                .all(|region| region.coverage() != UnitLoanActionCoverage::Complete)
        );
    }
}

fn assert_result_projections(observed: &Observed) {
    assert_units_admitted(observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let handoffs: Vec<_> = observation
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowHandoffCommitted(fact) => Some(fact.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(handoffs, unit.borrow_call_handoffs());
        let mut ends = Vec::new();
        for action in observation.actions() {
            if let UnitLoanAction::BorrowBindingEnd(fact) = action
                && !ends.contains(fact)
            {
                ends.push(*fact);
            }
        }
        assert_eq!(ends, unit.borrow_results().ends());
        assert_ne!(observation.coverage(), UnitLoanActionCoverage::Complete);
    }
}

#[test]
fn abandoned_initializer_has_no_committed_result_or_binding_end() {
    let observed = observe(
        "fun select(source: String, tail: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, return); println(item) }",
    );
    assert_result_projections(&observed);
    for unit in &observed.units {
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.borrow_results().ends().is_empty());
        assert_eq!(unit.borrow_call_prefix_ends().len(), 1);
        assert!(
            !unit.loan_action_observations()[0]
                .actions()
                .iter()
                .any(|action| matches!(
                    action,
                    UnitLoanAction::BorrowHandoffCommitted(_) | UnitLoanAction::BorrowBindingEnd(_)
                ))
        );
    }
}

#[test]
fn child_binding_retires_before_parent_and_actual_owner_drop() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = view(parent); println(child) }",
    );
    assert_result_projections(&observed);
    for unit in &observed.units {
        let bindings = unit.borrow_results().bindings();
        assert_eq!(bindings.len(), 2);
        let child = bindings
            .iter()
            .find(|binding| binding.parent().is_some())
            .unwrap();
        let parent = child.parent().unwrap();
        assert_eq!(
            unit.borrow_call_handoffs().len(),
            1,
            "do not manufacture an unsupported child handoff"
        );
        let actions = unit.loan_action_observations()[0].actions();
        let child_end = actions.iter().position(|action| matches!(action, UnitLoanAction::BorrowBindingEnd(end) if end.binding() == child.binding())).unwrap();
        let parent_end = actions.iter().position(|action| matches!(action, UnitLoanAction::BorrowBindingEnd(end) if end.binding() == parent)).unwrap();
        let UnitLoanTarget::Place(place) = child.origin() else {
            panic!("shared named owner");
        };
        let owner_drop = actions.iter().position(|action| matches!(action, UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::Drop(drop), .. } if drop.target() == UnitDropTarget::Named(place.root()))).unwrap();
        assert!(child_end < parent_end && parent_end < owner_drop);
    }
}

#[test]
fn siblings_keep_distinct_handoffs_and_protect_the_shared_source() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val first = view(owner); borrow val second = view(owner); println(first); println(second) }",
    );
    assert_result_projections(&observed);
    for unit in &observed.units {
        let [first, second] = unit.borrow_call_handoffs() else {
            panic!("two sibling commits");
        };
        assert_ne!(first.binding(), second.binding());
        assert_ne!(first.source_loan().call(), second.source_loan().call());
        assert_eq!(first.source_loan().target(), second.source_loan().target());
        let UnitLoanTarget::Place(place) = first.source_loan().target() else {
            panic!("shared root");
        };
        let observation = &unit.loan_action_observations()[0];
        let actions = observation.actions();
        let first_end = actions.iter().position(|action| matches!(action, UnitLoanAction::BorrowBindingEnd(end) if end.binding() == first.binding())).unwrap();
        let second_end = actions.iter().position(|action| matches!(action, UnitLoanAction::BorrowBindingEnd(end) if end.binding() == second.binding())).unwrap();
        let drop = actions.iter().position(|action| matches!(action, UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::Drop(drop), .. } if drop.target() == UnitDropTarget::Named(place.root()))).unwrap();
        assert!(first_end < second_end && second_end < drop);
        assert_ne!(
            observation.action_id(first_end),
            observation.action_id(second_end)
        );
    }
}

#[test]
fn historical_auxiliary_description_does_not_replay_an_end_at_commit() {
    let observed = observe(
        "fun select(source: String, aux: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, \"auxiliary\"); println(item) }",
    );
    assert_result_projections(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("actual commit");
        };
        let [aux] = handoff.auxiliary_loan_ends() else {
            panic!("historical auxiliary retirement");
        };
        let actions = unit.loan_action_observations()[0].actions();
        let drop = actions.iter().position(|action| matches!(action, UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::Drop(drop), .. } if drop.target() == UnitDropTarget::Temporary(aux.argument()))).unwrap();
        let commit = actions.iter().position(|action| matches!(action, UnitLoanAction::BorrowHandoffCommitted(fact) if fact == handoff)).unwrap();
        assert!(
            drop < commit,
            "historical descriptor cannot move the real auxiliary cleanup"
        );
        assert!(!actions.iter().any(|action| matches!(action, UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::EndCallLoan(loan), .. } if loan == aux)));
    }
}

#[test]
fn result_action_ids_remain_catalog_bound_across_reanalysis() {
    let left = observe(SOURCE);
    let right = observe(SOURCE);
    assert_result_projections(&left);
    assert_result_projections(&right);
    for (left, right) in left.units.iter().zip(&right.units) {
        let left = &left.loan_action_observations()[0];
        let right = &right.loan_action_observations()[0];
        let mut checked = 0;
        for (index, action) in left.actions().iter().enumerate() {
            if matches!(
                action,
                UnitLoanAction::BorrowHandoffCommitted(_) | UnitLoanAction::BorrowBindingEnd(_)
            ) {
                let id = left.action_id(index).unwrap();
                assert!(right.action(&id).is_none());
                checked += 1;
            }
        }
        assert_eq!(checked, 2);
    }
}
