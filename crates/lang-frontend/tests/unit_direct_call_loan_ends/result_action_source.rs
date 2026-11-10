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
