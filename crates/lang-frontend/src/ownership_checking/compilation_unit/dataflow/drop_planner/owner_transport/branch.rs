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
        if !self.expression(condition, DropExpressionUse::Read, state)? {
            return Ok(false);
        }
        let checkpoint = self.owner_observation.as_ref().and_then(|observation| {
            self.owner_observation_active()
                .then_some(observation.events.len())
        });
        let base = state.clone();
        let mut branch_states = Vec::new();
        let mut then_state = base.clone();
        let (then_tail, then_result) =
            self.evaluate_control_body(then_branch, branch_usage, &mut then_state)?;
        if then_result.continues {
            self.drop_branch_exit(id, 0, &mut then_state);
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
        let then_events = self.take_owner_edge(checkpoint);
        if let Some(else_branch) = else_branch {
            let mut else_state = base;
            let (else_tail, else_result) =
                self.evaluate_control_body(else_branch, branch_usage, &mut else_state)?;
            if else_result.continues {
                self.drop_branch_exit(id, 1, &mut else_state);
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
            self.drop_branch_exit(id, 1, &mut implicit);
            branch_states.push(implicit);
        }
        let else_events = self.take_owner_edge(checkpoint);
        if self.owner_observation_active() {
            if let (Some(then_events), Some(else_events), [left, right]) =
                (then_events, else_events, branch_states.as_slice())
            {
                // Cleanup has run on both paths. Common symbol presence alone is not
                // evidence that the same owner reaches the continuation.
                let same_owners = left.values.len() == right.values.len()
                    && left.values.iter().all(|value| {
                        value.owner.is_some()
                            && right
                                .position(value.symbol)
                                .is_some_and(|index| right.values[index].owner == value.owner)
                    });
                if same_owners {
                    self.owner_event(UnitOwnerTransportEvent::If {
                        control: self.checker.unit_expression(id),
                        then_events,
                        else_events,
                    });
                } else {
                    self.reject_owner_expression(id, UnitOwnerUnsupportedReason::MissingOwner);
                }
            } else {
                // A return path needs a distinct delivery/exit contract, not a missing edge.
                self.reject_owner_expression(id, UnitOwnerUnsupportedReason::Syntax);
            }
        }
        let continues = !branch_states.is_empty();
        *state = self.merge_resource_states(id, branch_states);
        Ok(continues)
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
