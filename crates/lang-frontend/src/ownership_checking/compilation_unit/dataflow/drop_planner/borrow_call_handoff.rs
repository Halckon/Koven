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
    pub(super) fn observe_borrow_call_handoff(&mut self, id: ExpressionId, state: &mut ValueState) {
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
        let source_loan = (*source_loan).clone();
        let binding = binding.binding();
        let mut auxiliary_loan_ends = Vec::new();
        // These exact tokens would otherwise be removed by the immediately following
        // call-local retain. Observe that retirement, not descriptor end spans.
        let mut index = 0;
        while index < state.pending_loans.len() {
            let UnitIterationCleanupAction::EndCallLoan(loan) = &state.pending_loans[index].0
            else {
                index += 1;
                continue;
            };
            if loan.call() != call_id
                || loan.argument() == source_loan.argument()
                || !self.observes_borrow_auxiliary(loan, state)
            {
                index += 1;
                continue;
            }
            let (UnitIterationCleanupAction::EndCallLoan(loan), _) =
                state.pending_loans.remove(index)
            else {
                unreachable!("matched actual pending token")
            };
            auxiliary_loan_ends.push(loan);
        }
        self.pending_borrow_handoffs
            .push(UnitBorrowCallHandoffFact {
                source_loan,
                binding,
                auxiliary_loan_ends,
            });
    }
    fn observes_borrow_auxiliary(
        &self,
        loan: &crate::ownership_checking::UnitLoanFact,
        state: &ValueState,
    ) -> bool {
        let UnitLoanTarget::Place(place) = loan.target() else {
            return false;
        };
        let root = place.root();
        let provider = self.checker.iterations.values().any(|plan| {
            plan.bindings().iter().any(|binding| binding.symbol() == root)
                || matches!(plan.source(), Some(UnitLoanTarget::Place(source)) if source.root() == root)
        }) || self.checker.map_iterations.values().any(|plan| {
            plan.bindings().iter().any(|binding| binding.symbol() == root)
                || matches!(plan.source(), Some(UnitLoanTarget::Place(source)) if source.root() == root)
        });
        loan.kind() == LoanKind::Shared
            && place.is_root()
            && !provider
            && self.checker.formed_owned_locals.contains(&root)
            && state.position(root).is_some()
            && !state.borrow_bindings.contains(&root)
            && self
                .checker
                .borrow_result_call(loan.argument().expression())
                .is_none()
            && self
                .checker
                .place(loan.argument().expression())
                .is_ok_and(|actual| actual.as_ref() == Some(place))
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
        let mut preceding_call_loan_ends = std::collections::BTreeMap::<_, usize>::new();
        let mut ends = Vec::new();
        for (action_index, (point, action)) in self.iteration_actions.iter().enumerate() {
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
            // Count every actual call-loan end before applying publication gates.
            let loan_count = preceding_call_loan_ends
                .entry(transfer.index())
                .or_default();
            let preceding = *loan_count;
            *loan_count += 1;
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
            if !self.borrow_source_unwind_actions.contains(&action_index) {
                continue;
            }
            ends.push(UnitBorrowCallPrefixEndFact {
                loan: loan.clone(),
                point: point.into_unit(self.checker.source_unit),
                preceding_drops: *count,
                preceding_call_loan_ends: preceding,
            });
        }
        ends
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn borrow_initializer_candidate(
        &self,
        initializer: ExpressionId,
    ) -> Option<(
        crate::type_checking::UnitExpressionId,
        crate::type_checking::UnitExpressionId,
    )> {
        let call_id = self.checker.unit_expression(initializer);
        let call = self.checker.typed.call(call_id)?;
        if call.receiver().is_some()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || !matches!(call.result_source(), CallableResultSource::Borrow(_))
        {
            return None;
        }
        let source = self.checker.borrowed_call_source(initializer)?;
        Some((call_id, self.checker.unit_expression(source)))
    }

    pub(super) fn is_observed_borrow_source_unwind(
        &self,
        point: super::PlannerDropPoint,
        action: &UnitIterationCleanupAction,
        state: &ValueState,
    ) -> bool {
        let super::PlannerDropPoint::ControlTransfer(transfer) = point else {
            return false;
        };
        if !self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(transfer)
            .is_ok_and(|node| matches!(node.payload(), crate::parser::Expression::Return { .. }))
        {
            return false;
        }
        let UnitIterationCleanupAction::EndCallLoan(loan) = action else {
            return false;
        };
        if !state
            .borrow_initializer_sources
            .contains(&(loan.call(), loan.argument()))
            || loan.kind() != LoanKind::Shared
        {
            return false;
        }
        let UnitLoanTarget::Place(place) = loan.target() else {
            return false;
        };
        // Typed argument mapping selects the candidate only. The actual evaluated
        // token, original operand place and still-live owner prove this observation.
        place.is_root()
            && self.checker.formed_owned_locals.contains(&place.root())
            && state.position(place.root()).is_some()
            && !state.borrow_bindings.contains(&place.root())
            && self
                .checker
                .place(loan.argument().expression())
                .is_ok_and(|actual| actual.as_ref() == Some(place))
    }
}
