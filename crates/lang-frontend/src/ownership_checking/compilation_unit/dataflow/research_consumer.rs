//! Actual consumer call-loan cleanup and normal-completion research adapters.
use super::{Checker, Flows, State};
use crate::ownership_checking::{
    BorrowLoopBinding, BorrowLoopExpression, LoanKind, UnitLoanTarget,
    research_borrow_frame as frames, research_borrow_loop as loops,
};
use crate::{ast::ExpressionId, parser::Expression};
type Prepared = (
    Option<loops::Prepared<State>>,
    Option<frames::Prepared<State>>,
);
type Released = (
    Option<loops::Released<State>>,
    Option<frames::Released<State>>,
);
impl Checker<'_> {
    pub(super) fn research_prepare_consumer(
        &self,
        call: ExpressionId,
        flows: &Flows,
    ) -> Option<Prepared> {
        let loop_active = loops::consumer_active();
        let frame_active = frames::consumer_active();
        if (!loop_active && !frame_active)
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
            if frame_active {
                frames::consumer_live(
                    binding,
                    Some(place),
                    state.non_owning.get(&binding),
                    state
                        .loans
                        .iter()
                        .filter(|loan| {
                            loan.owner == super::flow::ActiveLoanOwner::BorrowBinding(binding)
                        })
                        .cloned()
                        .collect(),
                );
            }
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
        let mut expected = self.resource_snapshot(state);
        expected.loans.retain(|loan| loan.owner != call_owner);
        let call = BorrowLoopExpression::Unit(self.unit_expression(call));
        let frame = frame_active
            .then(|| {
                frames::prepare_consumer(call, identities.clone(), expected.clone(), active.len())
            })
            .flatten();
        let loop_token = loop_active
            .then(|| loops::prepare_consumer(call, identities, expected, active.len()))
            .flatten();
        Some((loop_token, frame))
    }

    pub(super) fn research_release_consumer(
        &self,
        prepared: Option<Prepared>,
        flows: &Flows,
    ) -> Option<Released> {
        let (loop_token, frame) = prepared?;
        let actual = flows
            .next
            .as_ref()
            .map(|state| self.resource_snapshot(state));
        Some((
            loops::release_consumer(loop_token, actual.as_ref()),
            frames::release_consumer(frame, actual.as_ref()),
        ))
    }
    pub(super) fn research_complete_consumer(&self, released: Option<Released>, flows: &Flows) {
        let Some((loop_token, frame)) = released else {
            return;
        };
        let actual = flows
            .next
            .as_ref()
            .map(|state| self.resource_snapshot(state));
        loops::complete_consumer(
            loop_token,
            actual.as_ref(),
            self.diagnostics.is_empty() && flows.breaks.is_none() && flows.continues.is_none(),
        );
        frames::complete_consumer(
            frame,
            actual.as_ref(),
            self.diagnostics.is_empty() && flows.breaks.is_none() && flows.continues.is_none(),
        );
    }
}
