//! Actual condition-before, condition-after and pre-merge empty-loop research boundaries.
use super::{Checker, Flows, State};
use crate::{
    ast::StatementId,
    ownership_checking::{research_borrow_loop as carried, research_borrow_zero_loop as zero},
};
pub(super) struct LoopResearch {
    carried: Option<carried::Ticket<State>>,
    zero: Option<zero::Ticket<State>>,
    body: Option<crate::ownership_checking::research_borrow_consumer_body::LoopTicket<State>>,
}
impl Checker<'_> {
    pub(super) fn research_zero_header(
        &self,
        statement: StatementId,
        body: StatementId,
        state: &State,
    ) -> Option<zero::Guard> {
        if !zero::active()
            || !self.diagnostics.is_empty()
            || !state.loans.is_empty()
            || !state.borrow_bindings.is_empty()
        {
            return None;
        }
        let snapshot = self.research_resource_snapshot(state);
        if !self.research_simple_resources(&snapshot) {
            return None;
        }
        let span = zero::shape(self.parsed, statement, body, |condition| {
            self.research_boolean_parameter(condition)
        })?;
        zero::header(
            statement,
            body,
            span,
            &snapshot,
            true,
            (
                self.borrow_results.bindings.len(),
                self.borrow_results.ends.len(),
                self.diagnostics.len(),
            ),
        )
    }
    pub(super) fn research_zero_body_enter(
        &self,
        scope: StatementId,
        state: &State,
    ) -> Option<zero::BodyTicket<State>> {
        if !zero::active() {
            return None;
        }
        zero::enter_body(
            scope,
            &self.research_resource_snapshot(state),
            state.loans.is_empty() && state.borrow_bindings.is_empty(),
            (
                self.borrow_results.bindings.len(),
                self.borrow_results.ends.len(),
                self.diagnostics.len(),
            ),
        )
    }
    pub(super) fn research_zero_body_close(
        &self,
        ticket: Option<zero::BodyTicket<State>>,
        flows: &Flows,
    ) {
        if ticket.is_none() {
            return;
        }
        let exit = flows
            .next
            .as_ref()
            .map(|state| self.research_resource_snapshot(state));
        zero::close_body(
            ticket,
            exit.as_ref(),
            flows.breaks.is_none() && flows.continues.is_none(),
            flows
                .next
                .as_ref()
                .is_some_and(|state| state.loans.is_empty() && state.borrow_bindings.is_empty()),
            (
                self.borrow_results.bindings.len(),
                self.borrow_results.ends.len(),
                self.diagnostics.len(),
            ),
        );
    }
    pub(super) fn research_loop_start(
        &self,
        statement: Option<StatementId>,
        body: StatementId,
        state: &State,
    ) -> LoopResearch {
        let zero = if zero::active() {
            let snapshot = self.research_resource_snapshot(state);
            zero::condition(
                statement,
                &snapshot,
                state.loans.is_empty() && state.borrow_bindings.is_empty(),
                (
                    self.borrow_results.bindings.len(),
                    self.borrow_results.ends.len(),
                    self.diagnostics.len(),
                ),
            )
        } else {
            None
        };
        LoopResearch {
            body: self.research_body_loop_start(statement, body, state),
            carried: self.research_carried_loop_start(statement, body, state),
            zero,
        }
    }
    pub(super) fn research_loop_finish(&self, ticket: LoopResearch, flows: &Flows) {
        if ticket.zero.is_some() {
            let normal = flows
                .next
                .as_ref()
                .map(|state| self.research_resource_snapshot(state));
            let empty = flows
                .next
                .as_ref()
                .is_some_and(|state| state.loans.is_empty() && state.borrow_bindings.is_empty());
            zero::finish(
                ticket.zero,
                normal.as_ref(),
                flows.breaks.is_none() && flows.continues.is_none(),
                empty,
                (
                    self.borrow_results.bindings.len(),
                    self.borrow_results.ends.len(),
                    self.diagnostics.len(),
                ),
            );
        }
        self.research_body_finish(ticket.body, flows);
        self.research_carried_loop_finish(ticket.carried, flows);
    }
}
