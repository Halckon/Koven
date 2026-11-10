//! Preserve actual parameter-source tokens until a verified wrapper return executes.
use super::{DropPlanner, PlannerDropPoint, ValueState};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::{
        LoanKind, OwnershipBindingKind, UnitBorrowForwardTransportFact,
        UnitIterationCleanupAction as Action, UnitLoanFact, UnitLoanTarget,
    },
    parser::Expression,
    type_checking::{CallableResultSource, UnitCallTarget, UnitExpressionId},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingForwardTransport {
    returned: UnitExpressionId,
    source: UnitLoanFact,
}

impl DropPlanner<'_, '_> {
    pub(super) fn transport_borrow_forward_source(&self, id: ExpressionId, state: &mut ValueState) {
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
        let Some((forwarded, origin)) = self.forwarded_borrow_source(source_call) else {
            return;
        };
        let UnitLoanTarget::Place(place) = origin.origin() else {
            return;
        };
        // A borrowed parameter has no local owner to release. Do not generalize this
        // handoff to local aliases, fields, receiver roots or exclusive sources.
        // An unrelated active provider does not own this parameter token; an
        // overlapping active provider still remains outside this observation.
        if !place.is_root()
            || state.iterations.iter().any(|frame| {
                frame.source_root.is_some_and(|provider| {
                    provider == place.root()
                        || self
                            .checker
                            .borrow_results
                            .bindings()
                            .iter()
                            .any(|binding| {
                                // The checker already canonicalized this stable alias;
                                // field aliases protect the complete source root too.
                                binding.binding() == provider
                                    && matches!(binding.origin(), UnitLoanTarget::Place(origin)
                                        if origin.root() == place.root())
                            })
                })
            })
            || self.checker.symbol_kind(place.root()) != Some(SymbolKind::ValueParameter)
            || !self
                .checker
                .bindings
                .get(&place.root())
                .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Shared)
            || state.borrow_bindings.contains(&place.root())
        {
            return;
        }
        let Some(index) = state.pending_loans.iter().position(|(action, _)| {
            matches!(action, Action::EndCallLoan(loan)
                if loan.call() == forwarded.call()
                && loan.argument() == forwarded.argument()
                && loan.target() == origin.origin()
                && loan.kind() == LoanKind::Shared
                && self.checker.place(loan.argument().expression()).is_ok_and(|actual| actual.as_ref() == Some(place)))
        }) else { return; };
        // Remove the real token before the call-local retain can discard it. The
        // verified descriptor selects the token but never manufactures a transfer.
        let (Action::EndCallLoan(source), _) = state.pending_loans.remove(index) else {
            unreachable!("matched actual source token")
        };
        state
            .borrow_forward_transports
            .push(PendingForwardTransport {
                returned: origin.expression(),
                source,
            });
    }

    pub(super) fn finish_borrow_forward_transport(
        &mut self,
        point: PlannerDropPoint,
        state: &mut ValueState,
    ) {
        let PlannerDropPoint::ControlTransfer(expression) = point else {
            return;
        };
        let Ok(node) = self.checker.parsed.ast().expressions().get(expression) else {
            return;
        };
        let returned = match node.payload() {
            Expression::Return {
                value: Some(value), ..
            } => self.checker.unit_expression(*value),
            Expression::Return { value: None, .. } => return,
            // Expression-bodied functions use their result expression as the transfer point.
            _ => self.checker.unit_expression(expression),
        };
        let Some(index) = state
            .borrow_forward_transports
            .iter()
            .position(|pending| pending.returned == returned)
        else {
            return;
        };
        let PendingForwardTransport { returned, source } =
            state.borrow_forward_transports.remove(index);
        // Consume the transport state, not the live source loan. Ownership of the
        // token passes to the wrapper result; no loan-end or cleanup action is added.
        self.borrow_forward_transports
            .push(UnitBorrowForwardTransportFact {
                source,
                returned,
                point: point.into_unit(self.checker.source_unit),
            });
    }
}
