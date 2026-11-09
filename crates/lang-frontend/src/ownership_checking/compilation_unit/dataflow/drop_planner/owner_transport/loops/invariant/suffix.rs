//! Activate the real prefix collector only around the admitted suffix statement.
use super::*;

impl DropPlanner<'_, '_> {
    // This wrapper includes statement ASAP drops and the existing borrow-end wrapper.
    // Restore the ordinary Unsupported sink before propagating either success or error.
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn statement(
        &mut self,
        id: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let saved = self.begin_invariant_suffix(id, state);
        let result = self.statement_with_borrow_ends(id, state);
        if let Some((ordinary, entry)) = saved {
            let collected = self.owner_observation.replace(ordinary);
            if matches!(result, Ok(true))
                && empty_boundary(state)
                && self.loop_boundaries.is_empty()
                && self.owner_branch_depth == 0
                && self.owner_choice.is_none()
                && let Some(transaction) = &mut self.owner_loop
                && let Some(input) = &transaction.invariant
                && id == input.suffix
                && input
                    .expected
                    .as_ref()
                    .is_some_and(|value| value.scope_depth == self.scope_depth)
                && let Some(InvariantLoopValueV1::Backedge(boundaries)) =
                    &mut transaction.invariant_result
                && let Some(graph) =
                    collected.and_then(|observation| observation.value_observation_v1())
            {
                boundaries.observe_post_loop(
                    UnitStatementId::new(self.checker.source_unit, id),
                    self.checker.unit_expression(input.use_expression),
                    input.retired,
                    entry,
                    graph,
                );
            }
        }
        result
    }

    fn begin_invariant_suffix(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) -> Option<(UnitOwnerObservation, UnitOwnerLoopBindingV1)> {
        if !self.owner_observation_enabled
            || !self.loop_boundaries.is_empty()
            || self.owner_branch_depth != 0
            || self.owner_choice.is_some()
        {
            return None;
        }
        let transaction = self.owner_loop.as_mut()?;
        let input = transaction.invariant.as_mut()?;
        let InvariantLoopValueV1::Backedge(boundaries) = transaction.invariant_result.as_ref()?
        else {
            return None;
        };
        if id != input.suffix
            || transaction.expected != LoopBodyCompletion::Fallthrough
            || !boundaries.is_consistent()
            || boundaries.post_loop.is_some()
            || input
                .expected
                .as_ref()
                .is_none_or(|value| value.scope_depth != self.scope_depth)
        {
            return None;
        }
        let entry = input.snapshot(state)?;
        if entry != *boundaries.exit() {
            return None;
        }
        let ordinary = self.owner_observation.as_mut()?;
        if !matches!(ordinary.status, UnitOwnerObservationStatus::Unsupported {
            point: crate::ownership_checking::UnitDropPoint::AfterStatement(statement),
            reason: UnitOwnerUnsupportedReason::Syntax,
        } if statement == boundaries.statement())
            || ordinary.callable != transaction.callable
            || !std::sync::Arc::ptr_eq(&ordinary.identity, &transaction.identity)
            || !ordinary.definitions.is_empty()
            || !ordinary.events.is_empty()
        {
            return None;
        }
        let prefix = input.prefix.take()?;
        Some((std::mem::replace(ordinary, prefix), entry))
    }
}
