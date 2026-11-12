//! Real state boundaries, independent of the old loop-owner observation status.
use super::*;

pub(super) struct Observation {
    token: action_source::LoopInvariantToken,
    owner: Option<OwnedValue>,
    root: UnitSymbolId,
}
impl DropPlanner<'_, '_> {
    pub(super) fn begin_loop_invariant_observation(
        &self,
        id: StatementId,
        condition: ExpressionId,
        body: StatementId,
        state: &ValueState,
    ) -> Option<Observation> {
        let plan = self.loop_invariant_plan(id, condition, body, state)?;
        let [pending] = state.borrow_binding_transports.as_slice() else {
            return None;
        };
        let (source, begin, handoff) = pending.loop_obligation(plan.binding)?;
        let crate::ownership_checking::UnitLoanTarget::Place(place) = source.target() else {
            return None;
        };
        if !place.is_root() {
            return None;
        }
        let root = place.root();
        let owner = state
            .values
            .iter()
            .find(|value| value.symbol == root)
            .cloned();
        let token = self.iteration_actions.begin_loop_invariant(
            plan,
            source.clone(),
            begin.clone(),
            handoff.clone(),
        )?;
        let result = Observation { token, owner, root };
        self.loop_invariant_state_matches(&result, state)
            .then_some(result)
    }
    fn loop_invariant_state_matches(&self, observation: &Observation, state: &ValueState) -> bool {
        let token = &observation.token;
        if state.borrow_bindings.as_slice() != [token.plan.binding]
            || !state.pending_borrow_results.is_empty()
            || !state.pending_loans.is_empty()
            || !state.pending_temporaries.is_empty()
            || !state.pending_borrows.is_empty()
            || !state.pending_capture_borrows.is_empty()
            || state.values.iter().find(|v| v.symbol == observation.root)
                != observation.owner.as_ref()
        {
            return false;
        }
        let [pending] = state.borrow_binding_transports.as_slice() else {
            return false;
        };
        pending
            .loop_obligation(token.plan.binding)
            .is_some_and(|(source, begin, handoff)| {
                source == &token.source
                    && begin.same_record(&token.begin)
                    && handoff.same_record(&token.handoff)
            })
    }
    pub(super) fn advance_loop_invariant_observation(
        &self,
        observation: &mut Option<Observation>,
        state: &ValueState,
        phase: usize,
        normal: bool,
    ) {
        let valid = observation
            .as_ref()
            .is_some_and(|o| normal && self.loop_invariant_state_matches(o, state));
        if !valid {
            *observation = None;
            return;
        }
        if let Some(o) = observation
            && !self
                .iteration_actions
                .advance_loop_invariant(&mut o.token, phase)
        {
            *observation = None;
        }
    }
    pub(super) fn complete_loop_invariant_observation(
        &mut self,
        mut observation: Option<Observation>,
        state: &ValueState,
    ) {
        self.advance_loop_invariant_observation(&mut observation, state, 3, true);
        if let Some(o) = observation {
            self.iteration_actions.complete_loop_invariant(o.token);
        }
    }
}
