//! Move real pending source tokens across inner return; do not reconstruct ends from facts.
use super::{DropPlanner, ValueState};
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        LoanKind, UnitBorrowCallTransportFact, UnitIterationCleanupAction as Action, UnitLoanFact,
    },
    type_checking::{CallableResultSource, UnitCallTarget, UnitExpressionId},
};

#[derive(Clone, Debug)]
pub(super) struct PendingTransport {
    consumer: UnitExpressionId,
    argument: UnitExpressionId,
    source: UnitLoanFact,
    transfer: Option<super::action_source::ConsumerTransferHandle>,
}

// Observation metadata must never change the old semantic branch-merge equivalence.
impl PartialEq for PendingTransport {
    fn eq(&self, other: &Self) -> bool {
        self.consumer == other.consumer
            && self.argument == other.argument
            && self.source == other.source
    }
}
impl Eq for PendingTransport {}
impl PendingTransport {
    pub(super) fn reconcile_transfer(&mut self, alternatives: &[Self]) {
        for other in alternatives.iter().filter(|other| *other == self) {
            let same = self
                .transfer
                .as_ref()
                .zip(other.transfer.as_ref())
                .is_some_and(|(left, right)| left.same_record(right));
            if !same {
                self.transfer = None;
                break;
            }
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn transport_borrow_call_source(
        &mut self,
        id: ExpressionId,
        state: &mut ValueState,
    ) {
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
        for use_fact in self
            .checker
            .borrow_results
            .call_uses()
            .iter()
            .filter(|fact| fact.expression() == source_call)
            .filter(|fact| {
                fact.end_point()
                    == crate::ownership_checking::UnitDropPoint::CallReturn(fact.call())
            })
        {
            let Some(consumer) = self.checker.typed.call(use_fact.call()) else {
                continue;
            };
            if consumer.receiver().is_some()
                || !matches!(
                    consumer.target(),
                    UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
                )
                || consumer.result_source() != CallableResultSource::Owned
            {
                continue;
            }
            let source = use_fact.source_loan();
            let Some(index) = state.pending_loans.iter().position(|(action, _)| {
                matches!(action, Action::EndCallLoan(loan)
                    if loan.call() == source.call()
                    && loan.argument() == source.argument()
                    && loan.target() == use_fact.origin()
                    && self.observes_borrow_auxiliary(loan, state))
            }) else {
                continue;
            };
            // This is the exact token that the call-local retain would otherwise discard.
            // Move it, rather than copying a checker descriptor into a claimed end event.
            let (Action::EndCallLoan(source), _) = state.pending_loans.remove(index) else {
                unreachable!("matched actual source token")
            };
            let transfer = self.iteration_actions.transfer_consumer(
                &source,
                use_fact.call(),
                use_fact.argument(),
            );
            state.borrow_call_transports.push(PendingTransport {
                consumer: use_fact.call(),
                argument: use_fact.argument(),
                transfer: Some(transfer),
                source,
            });
        }
    }

    pub(super) fn finish_borrow_call_transports(
        &mut self,
        id: ExpressionId,
        state: &mut ValueState,
    ) {
        let call = self.checker.unit_expression(id);
        // Only normal completion reaches here. Consume actual outer tokens in the
        // same reverse-operand order as ordinary pending retirement.
        for index in (0..state.pending_loans.len()).rev() {
            let Action::EndCallLoan(consumer) = &state.pending_loans[index].0 else {
                continue;
            };
            if consumer.call() != call || consumer.kind() != LoanKind::Shared {
                continue;
            }
            let Some(transport_index) = state.borrow_call_transports.iter().position(|pending| {
                pending.consumer == call
                    && pending.argument == consumer.argument()
                    && pending.source.target() == consumer.target()
            }) else {
                continue;
            };
            let (Action::EndCallLoan(consumer), _) = state.pending_loans.remove(index) else {
                unreachable!("matched actual consumer token")
            };
            let PendingTransport {
                source, transfer, ..
            } = state.borrow_call_transports.remove(transport_index);
            // The consumer retires first, then releases its transported ancestor.
            // This side product does not add iteration/native cleanup actions.
            self.iteration_actions
                .retire_consumer(transfer, UnitBorrowCallTransportFact { source, consumer });
        }
    }
}

#[cfg(test)]
mod tests;
