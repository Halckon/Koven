//! Observe the existing normal-return token retirement without changing its cleanup.
use super::{DropPlanner, ValueState};
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        LoanKind, UnitBorrowCallHandoffFact, UnitIterationCleanupAction, UnitLoanTarget,
    },
    type_checking::{CallableResultSource, UnitCallTarget},
};

impl DropPlanner<'_, '_> {
    pub(super) fn observe_borrow_call_handoff(&mut self, id: ExpressionId, state: &ValueState) {
        let call_id = self.checker.unit_expression(id);
        let Some(call) = self.checker.typed.call(call_id) else {
            return;
        };
        if call.receiver().is_some()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || !matches!(call.result_source(), CallableResultSource::Borrow(_))
        {
            return;
        }
        let Some(binding) = self
            .checker
            .borrow_results
            .bindings()
            .iter()
            .find(|binding| {
                binding.initializer() == call_id
                    && binding.parent().is_none()
                    && state.pending_borrow_results.contains(&binding.binding())
                    && binding
                        .source_loan()
                        .is_some_and(|source| source.call() == call_id)
            })
        else {
            return;
        };
        let source = binding.source_loan().expect("checked source");
        let loans: Vec<_> = state
            .pending_loans
            .iter()
            .rev()
            .filter_map(|(action, _)| match action {
                UnitIterationCleanupAction::EndCallLoan(loan) if loan.call() == call_id => {
                    Some(loan.clone())
                }
                _ => None,
            })
            .collect();
        // Bound this observation to root Shared places. No receiver, projection,
        // temporary, exclusive source, or carrier capability is implied.
        if loans.iter().any(|loan| {
            loan.kind() != LoanKind::Shared
                || !matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root())
        }) {
            return;
        }
        let matching: Vec<_> = loans
            .iter()
            .filter(|loan| loan.argument() == source.argument())
            .collect();
        let [source_loan] = matching.as_slice() else {
            return;
        };
        if source_loan.target() != binding.origin() {
            return;
        }
        let UnitLoanTarget::Place(place) = source_loan.target() else {
            return;
        };
        if !self.checker.formed_owned_locals.contains(&place.root())
            || state.position(place.root()).is_none()
        {
            return;
        }
        self.pending_borrow_handoffs
            .push(UnitBorrowCallHandoffFact {
                source_loan: (*source_loan).clone(),
                binding: binding.binding(),
            });
    }
    pub(super) fn commit_borrow_call_handoff(
        &mut self,
        binding: crate::name_resolution::UnitSymbolId,
        continues: bool,
    ) {
        let mut remaining = Vec::new();
        for fact in self.pending_borrow_handoffs.drain(..) {
            if fact.binding() == binding {
                if continues {
                    self.borrow_call_handoffs.push(fact);
                }
            } else {
                remaining.push(fact);
            }
        }
        self.pending_borrow_handoffs = remaining;
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn borrow_call_prefix_ends(
        &self,
    ) -> Vec<crate::ownership_checking::UnitBorrowCallPrefixEndFact> {
        use super::PlannerDropPoint;
        use crate::ownership_checking::UnitBorrowCallPrefixEndFact;
        let mut preceding_drops = std::collections::BTreeMap::new();
        let mut ends = Vec::new();
        for (point, action) in &self.iteration_actions {
            let PlannerDropPoint::ControlTransfer(transfer) = point else {
                continue;
            };
            if !self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(*transfer)
                .is_ok_and(|node| {
                    matches!(node.payload(), crate::parser::Expression::Return { .. })
                })
            {
                continue;
            }
            let count = preceding_drops.entry(transfer.index()).or_insert(0);
            let loan = match action {
                UnitIterationCleanupAction::Drop(_) => {
                    *count += 1;
                    continue;
                }
                UnitIterationCleanupAction::EndCallLoan(loan) => loan,
                _ => continue,
            };
            let Some(call) = self.checker.typed.call(loan.call()) else {
                continue;
            };
            let UnitLoanTarget::Place(place) = loan.target() else {
                continue;
            };
            if loan.kind() != LoanKind::Shared
                || !place.is_root()
                || !self.checker.formed_owned_locals.contains(&place.root())
                || call.receiver().is_some()
                || !matches!(call.target(), UnitCallTarget::Declaration(_))
                || !matches!(call.result_source(), CallableResultSource::Borrow(_))
            {
                continue;
            }
            // A final binding proves only the supported normal path. Execution of
            // this abandoned path is independently witnessed by the actual action.
            if !self
                .checker
                .borrow_results
                .bindings()
                .iter()
                .any(|binding| {
                    binding.initializer() == loan.call()
                        && binding.parent().is_none()
                        && binding.origin() == loan.target()
                        && binding.source_loan().is_some_and(|source| {
                            source.call() == loan.call() && source.argument() == loan.argument()
                        })
                })
            {
                continue;
            }
            ends.push(UnitBorrowCallPrefixEndFact {
                loan: loan.clone(),
                point: point.into_unit(self.checker.source_unit),
                preceding_drops: *count,
            });
        }
        ends
    }
}
