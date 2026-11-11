//! Observe the existing If transitions without changing cleanup or manufacturing a phi.
use super::*;

impl DropPlanner<'_, '_> {
    pub(in super::super) fn owner_if_expression(
        &mut self,
        id: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let branch_usage = self.control_result_usage(id);
        self.observe_branch_condition(id, condition, state, false);
        let condition_result = self.expression(condition, DropExpressionUse::Read, state);
        if !matches!(condition_result, Ok(true)) {
            state.branch_forward_plan = None;
            if state
                .forward_scope
                .is_some_and(|scope| scope.branch.is_some())
            {
                state.forward_scope = None;
            }
            return condition_result;
        }
        // Restore depth even when legacy traversal returns an error. Only the real
        // branch bodies and their cleanup run inside this admission boundary.
        let outcomes = self.checker.ordinary_condition_outcomes(condition);
        self.observe_branch_condition(id, condition, state, true);
        self.prepare_forward_split(id, condition, outcomes, state);
        self.begin_owner_loop_split(id, condition, outcomes, state);
        self.conditional_break_split_enter(id, condition, outcomes, state);
        self.conditional_return_split_enter(id, condition, outcomes, state);
        self.owner_branch_depth += 1;
        let result = self.owner_if_edges(
            id,
            (then_branch, else_branch),
            outcomes,
            branch_usage,
            state,
        );
        self.owner_branch_depth -= 1;
        if result.is_err() {
            state.branch_forward_plan = None;
            if state
                .forward_scope
                .is_some_and(|scope| scope.branch.is_some())
            {
                state.forward_scope = None;
            }
        }
        result
    }

    fn owner_if_edges(
        &mut self,
        id: ExpressionId,
        (then_branch, else_branch): (StatementId, Option<StatementId>),
        (reaches_then, reaches_else): (bool, bool),
        branch_usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let checkpoint = self.owner_observation.as_ref().and_then(|observation| {
            self.owner_observation_active()
                .then_some(observation.events.len())
        });
        let base = state.clone();
        let branch_plan = base.branch_forward_plan;
        let mut branch_states = Vec::new();
        let mut then_terminated = false;
        if reaches_then {
            let mut then_state = base.clone();
            self.enter_forward_arm(id, true, &mut then_state);
            self.drop_branch_entry(id, 0, &[], &mut then_state);
            self.begin_owner_loop_arm(id, 0, then_branch, &then_state);
            self.conditional_break_arm_enter(id, then_branch, &then_state);
            self.conditional_return_arm_enter(id, then_branch, &then_state);
            self.enter_root_if_arm(id, 0);
            let evaluated = self.evaluate_control_body(then_branch, branch_usage, &mut then_state);
            self.leave_root_if_arm();
            let (then_tail, then_result) = evaluated?;
            then_terminated = !then_result.continues;
            if then_result.continues {
                self.drop_branch_exit(id, 0, &mut then_state);
            }
            self.finish_owner_loop_arm(id, 0, then_tail, &then_result, &then_state);
            self.conditional_break_arm_finish(id, then_tail, &then_result, &then_state);
            self.conditional_return_arm_finish(id, then_tail, &then_result, &then_state);
            if then_result.continues {
                branch_states.push(then_state);
            }
            if let Some(choice) = &mut self.owner_choice {
                choice.edge_completed(
                    self.checker.unit_expression(id),
                    0,
                    then_tail.map(|tail| self.checker.unit_expression(tail)),
                    then_result.continues,
                );
            }
        }
        let then_events = self.take_owner_edge(checkpoint);
        if reaches_else {
            if let Some(else_branch) = else_branch {
                let mut else_state = base;
                self.drop_branch_entry(id, 1, &[], &mut else_state);
                self.begin_owner_loop_arm(id, 1, else_branch, &else_state);
                self.enter_root_if_arm(id, 1);
                let evaluated =
                    self.evaluate_control_body(else_branch, branch_usage, &mut else_state);
                self.leave_root_if_arm();
                let (else_tail, else_result) = evaluated?;
                if else_result.continues {
                    self.drop_branch_exit(id, 1, &mut else_state);
                }
                self.finish_owner_loop_arm(id, 1, else_tail, &else_result, &else_state);
                if else_result.continues {
                    branch_states.push(else_state);
                }
                if let Some(choice) = &mut self.owner_choice {
                    choice.edge_completed(
                        self.checker.unit_expression(id),
                        1,
                        else_tail.map(|tail| self.checker.unit_expression(tail)),
                        else_result.continues,
                    );
                }
            } else {
                // Legacy cleanup still executes on this real edge, even though this slice
                // does not admit implicit-else observations.
                let mut implicit = base;
                if then_terminated {
                    self.enter_forward_arm(id, false, &mut implicit);
                } else {
                    implicit.branch_forward_plan = None;
                    if implicit
                        .forward_scope
                        .is_some_and(|scope| scope.branch.is_some())
                    {
                        implicit.forward_scope = None;
                    }
                }
                self.conditional_break_implicit(id, false, &implicit);
                self.conditional_return_implicit(id, false, &implicit);
                self.drop_branch_entry(id, 1, &[], &mut implicit);
                self.drop_branch_exit(id, 1, &mut implicit);
                self.conditional_break_implicit(id, true, &implicit);
                self.conditional_return_implicit(id, true, &implicit);
                branch_states.push(implicit);
            }
        }
        let else_events = self.take_owner_edge(checkpoint);
        if self.owner_observation_active() {
            if let (Some(then_events), Some(else_events)) = (then_events, else_events) {
                // A terminated arm has no continuation state. Its explicit return
                // event and complete cleanup are checked by both replays.
                let same_owners = match branch_states.as_slice() {
                    // No live join is needed when both arms terminate. The event
                    // validators require explicit terminal cleanup on every leaf.
                    [] | [_] => true,
                    [left, right] => {
                        left.values.len() == right.values.len()
                            && left.values.iter().all(|value| {
                                value.owner.is_some()
                                    && right.position(value.symbol).is_some_and(|index| {
                                        right.values[index].owner == value.owner
                                    })
                            })
                    }
                    _ => false,
                };
                if same_owners {
                    if !self.conditional_return_split_finish(
                        id,
                        &then_events,
                        &else_events,
                        branch_states.len(),
                    ) && !self.conditional_break_split_finish(
                        id,
                        &then_events,
                        &else_events,
                        branch_states.len(),
                    ) && !self.finish_owner_loop_split(
                        id,
                        &then_events,
                        &else_events,
                        branch_states.len(),
                    ) {
                        self.owner_event(UnitOwnerTransportEvent::If {
                            control: self.checker.unit_expression(id),
                            then_events,
                            else_events,
                        });
                    }
                } else {
                    self.reject_owner_expression(id, UnitOwnerUnsupportedReason::MissingOwner);
                }
            } else {
                // Unsupported edges cannot publish a partial trace.
                self.reject_owner_expression(id, UnitOwnerUnsupportedReason::Syntax);
            }
        }
        let continues = !branch_states.is_empty();
        self.observe_branch_join(branch_plan, then_terminated, &branch_states);
        *state = self.merge_resource_states(id, branch_states);
        self.finish_owner_loop_join(id, state);
        self.conditional_break_if_completed(id, continues, state);
        self.conditional_return_if_completed(id, continues, state);
        Ok(continues)
    }

    // Keep legacy Return traversal and cleanup in the same order, independent
    // of whether this observation is enabled or supported.
    pub(in super::super) fn owner_return_expression(
        &mut self,
        id: ExpressionId,
        value: Option<ExpressionId>,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        self.prepare_forward_return(id, value, state);
        let result = self.owner_return_expression_inner(id, value, state);
        if state
            .forward_scope
            .is_some_and(|scope| scope.branch.is_some())
        {
            state.forward_scope = None;
        }
        state.branch_forward_plan = None;
        result
    }

    fn owner_return_expression_inner(
        &mut self,
        id: ExpressionId,
        value: Option<ExpressionId>,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let return_consumer = if let Some(value) = value {
            let result =
                self.evaluate_owner_expression(value, DropExpressionUse::Consume, state)?;
            if !result.continues {
                return Ok(false);
            }
            self.deliver_conditional_return(id, value, result.owner.as_ref(), state);
            self.deliver_open_loop_return(id, value, result.owner.as_ref(), state);
            // Preserve the actual payload carrier before the existing cleanup, not a P projection.
            self.deliver_replace_return_consumer(id, value, result, state)
        } else {
            None
        };
        self.observe_branch_return(state, false);
        self.finish_return_iterations(id, state);
        self.drop_all(PlannerDropPoint::ControlTransfer(id), state);
        self.finish_conditional_return(id, state);
        self.finish_open_loop_return(id, state);
        if let Some(consumer) = return_consumer
            && let Some(transaction) = self.owner_loop.as_mut()
        {
            transaction.complete_replace_return_consumer(consumer);
        }
        if self.owner_branch_depth != 0 {
            if value.is_none() {
                self.owner_event(UnitOwnerTransportEvent::UnitReturn {
                    expression: self.checker.unit_expression(id),
                });
            } else if !self.conditional_return_cleaned(id) {
                self.reject_owner_expression(id, UnitOwnerUnsupportedReason::Syntax);
            }
        }
        self.observe_branch_return(state, true);
        Ok(false)
    }

    fn take_owner_edge(
        &mut self,
        checkpoint: Option<usize>,
    ) -> Option<Vec<UnitOwnerTransportEvent>> {
        if !self.owner_observation_active() {
            return None;
        }
        let checkpoint = checkpoint?;
        let observation = self.owner_observation.as_mut()?;
        Some(observation.events.split_off(checkpoint))
    }
}
