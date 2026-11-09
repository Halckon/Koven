//! Carry actual pending source tokens through explicit local borrow bindings.
use super::{DropPlanner, PlannerDropPoint, ValueState};
use crate::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitBorrowBindingTransportFact, UnitIterationCleanupAction as Action, UnitLoanFact,
    },
    type_checking::{CallableResultSource, UnitCallTarget},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingBindingTransport {
    binding: UnitSymbolId,
    source: UnitLoanFact,
}

impl DropPlanner<'_, '_> {
    pub(super) fn transport_borrow_binding_source(&self, id: ExpressionId, state: &mut ValueState) {
        let source_call = self.checker.unit_expression(id);
        let Some(call) = self.checker.typed.call(source_call) else {
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
                binding.initializer() == source_call
                    && binding.parent().is_none()
                    && state.pending_borrow_results.contains(&binding.binding())
            })
        else {
            return;
        };
        let Some(source) = binding.source_loan() else {
            return;
        };
        let Some(index) = state.pending_loans.iter().position(|(action, _)| {
            matches!(action, Action::EndCallLoan(loan)
                if loan.call() == source.call()
                && loan.argument() == source.argument()
                && loan.target() == binding.origin()
                && self.observes_borrow_auxiliary(loan, state))
        }) else {
            return;
        };
        // The normal-return retain must not discard this token. Move it intact;
        // the binding endpoint alone cannot stand in for a source-token end.
        let (Action::EndCallLoan(source), _) = state.pending_loans.remove(index) else {
            unreachable!("matched actual source token")
        };
        state
            .borrow_binding_transports
            .push(PendingBindingTransport {
                binding: binding.binding(),
                source,
            });
    }

    pub(super) fn finish_borrow_binding_transport(
        &mut self,
        binding: UnitSymbolId,
        point: PlannerDropPoint,
        state: &mut ValueState,
    ) {
        // Invoked only after an actual initialized binding is removed. Existing
        // child-before-parent termination and owner cleanup remain authoritative.
        let Some(index) = state
            .borrow_binding_transports
            .iter()
            .position(|pending| pending.binding == binding)
        else {
            return;
        };
        let PendingBindingTransport { source, binding } =
            state.borrow_binding_transports.remove(index);
        self.borrow_binding_transports
            .push(UnitBorrowBindingTransportFact {
                source,
                binding,
                point: point.into_unit(self.checker.source_unit),
            });
    }
}
