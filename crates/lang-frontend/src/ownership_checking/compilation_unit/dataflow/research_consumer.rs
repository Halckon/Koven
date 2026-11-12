//! Actual consumer call-loan cleanup and normal-completion research adapters.
use super::{Checker, Flows, State};
use crate::ownership_checking::{
    BorrowLoopBinding, BorrowLoopExpression, LoanKind, UnitLoanTarget,
    research_borrow_loop as loops,
};
use crate::{ast::ExpressionId, parser::Expression};
impl Checker<'_> {
    pub(super) fn research_prepare_consumer(
        &self,
        call: ExpressionId,
        flows: &Flows,
    ) -> Option<loops::Prepared<State>> {
        if !loops::consumer_active()
            || !self.diagnostics.is_empty()
            || flows.breaks.is_some()
            || flows.continues.is_some()
            || !self.research_direct_string_call(call, false)
        {
            return None;
        }
        let state = flows.next.as_ref()?;
        let Expression::Call { arguments, .. } =
            self.parsed.ast().expressions().get(call).ok()?.payload()
        else {
            return None;
        };
        let call_owner = super::flow::ActiveLoanOwner::Call(self.unit_expression(call));
        let active = state
            .loans
            .iter()
            .filter(|loan| loan.owner == call_owner)
            .collect::<Vec<_>>();
        if active.len() != arguments.len() || active.is_empty() {
            return None;
        }
        let mut identities = Vec::new();
        let mut matched = Vec::new();
        for argument in arguments {
            let node = self.parsed.ast().expressions().get(argument.value).ok()?;
            let binding = self.reference_symbol(node.span())?;
            let place = state.borrow_bindings.get(&binding)?;
            // The fact is selected by exact original call/argument IDs before relating its
            // origin to an actual active loan. Span alone never selects the transaction.
            let facts = self
                .loans
                .iter()
                .filter(|fact| {
                    fact.call() == self.unit_expression(call)
                        && fact.argument() == self.unit_expression(argument.value)
                })
                .collect::<Vec<_>>();
            let [fact] = facts.as_slice() else {
                return None;
            };
            if fact.kind() != LoanKind::Shared
                || fact.target() != &UnitLoanTarget::Place(place.clone())
            {
                return None;
            }
            let loans = active
                .iter()
                .enumerate()
                .filter(|(_, loan)| {
                    loan.kind == LoanKind::Shared
                        && loan.origin == fact.begin_span()
                        && loan.target == super::flow::ActiveLoanTarget::Place(place.clone())
                        && !loan.reserved
                })
                .collect::<Vec<_>>();
            let [(index, _)] = loans.as_slice() else {
                return None;
            };
            if matched.contains(index) {
                return None;
            }
            matched.push(*index);
            identities.push((
                BorrowLoopExpression::Unit(self.unit_expression(argument.value)),
                BorrowLoopBinding::Unit(binding),
            ));
        }
        let mut expected = self.research_resource_snapshot(state);
        expected.loans.retain(|loan| loan.owner != call_owner);
        loops::prepare_consumer(
            BorrowLoopExpression::Unit(self.unit_expression(call)),
            identities,
            expected,
            active.len(),
        )
    }
    pub(super) fn research_release_consumer(
        &self,
        prepared: Option<loops::Prepared<State>>,
        flows: &Flows,
    ) -> Option<loops::Released<State>> {
        let prepared = prepared?;
        let actual = flows
            .next
            .as_ref()
            .map(|state| self.research_resource_snapshot(state));
        loops::release_consumer(Some(prepared), actual.as_ref())
    }
    pub(super) fn research_complete_consumer(
        &self,
        released: Option<loops::Released<State>>,
        flows: &Flows,
    ) {
        let Some(released) = released else { return };
        let actual = flows
            .next
            .as_ref()
            .map(|state| self.research_resource_snapshot(state));
        loops::complete_consumer(
            Some(released),
            actual.as_ref(),
            self.diagnostics.is_empty() && flows.breaks.is_none() && flows.continues.is_none(),
        );
    }
}
