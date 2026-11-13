//! Observe the genuine no-else edge; never manufacture an arm body or expression.
use super::*;

impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn conditional_return_expression_supported(
        &self,
        id: ExpressionId,
    ) -> bool {
        let Some(t) = self.owner_loop.as_ref() else {
            return false;
        };
        let LoopBodyCompletion::ConditionalReturn { control, .. } = t.expected else {
            return false;
        };
        let Some((identity, boundary)) = &t.active else {
            return false;
        };
        control == self.checker.unit_expression(id)
            && exact_type(self.checker, id, BuiltinType::Unit)
            && t.normal.as_ref().is_some_and(|n| {
                n.valid && n.entered && n.assigned && n.conditional_return.is_some()
            })
            && !t.split_entered
            && t.completed.is_none()
            && t.active_branch.is_none()
            && t.pending_jump.is_none()
            && t.branch_completed == [false, false]
            && self.owner_branch_depth == 0
            && self.scope_depth == boundary + 1
            && self.loop_boundaries.as_slice() == [*boundary]
            && self.owner_observation.as_ref().is_some_and(|o| {
                o.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(identity, &o.identity)
            })
    }

    pub(in super::super::super) fn conditional_return_split_enter(
        &mut self,
        control: ExpressionId,
        condition: ExpressionId,
        outcomes: (bool, bool),
        state: &ValueState,
    ) {
        if outcomes != (true, true) || self.owner_branch_depth != 0 {
            return;
        }
        let Some(t) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::ConditionalReturn {
            control: expected,
            condition: expected_condition,
            ..
        } = t.expected
        else {
            return;
        };
        let Some((identity, boundary)) = &t.active else {
            return;
        };
        let Some(n) = &mut t.normal else {
            return;
        };
        if expected != self.checker.unit_expression(control)
            || expected_condition != self.checker.unit_expression(condition)
            || t.split_entered
            || t.completed.is_some()
            || !n.valid
            || !n.entered
            || !n.assigned
            || self.loop_boundaries.as_slice() != [*boundary]
            || self.scope_depth != boundary + 1
            || !self.owner_observation.as_ref().is_some_and(|o| {
                o.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(identity, &o.identity)
            })
        {
            return;
        }
        if n.conditional_return
            .as_mut()
            .is_some_and(|i| i.record(0, identity, state))
        {
            t.split_entered = true;
        } else {
            n.valid = false;
        }
    }
    pub(in super::super::super) fn conditional_return_arm_enter(
        &mut self,
        control: ExpressionId,
        body: StatementId,
        state: &ValueState,
    ) {
        let Some(t) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::ConditionalReturn {
            control: expected,
            body: expected_body,
            ..
        } = t.expected
        else {
            return;
        };
        if expected != self.checker.unit_expression(control)
            || expected_body != UnitStatementId::new(self.checker.source_unit, body)
            || !t.split_entered
            || t.active_branch.is_some()
            || t.pending_jump.is_some()
            || t.branch_completed != [false, false]
            || self.owner_branch_depth != 1
        {
            return;
        }
        let Some((identity, _)) = &t.active else {
            return;
        };
        let Some(n) = &mut t.normal else {
            return;
        };
        if n.conditional_return
            .as_mut()
            .is_some_and(|i| i.record(1, identity, state))
        {
            t.active_branch = Some(0);
        } else {
            n.valid = false;
        }
    }
    pub(in super::super::super) fn conditional_return_arm_finish(
        &mut self,
        control: ExpressionId,
        tail: Option<ExpressionId>,
        result: &EvaluatedOwner,
        state: &ValueState,
    ) {
        let Some(t) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::ConditionalReturn {
            control: expected,
            expression,
            ..
        } = t.expected
        else {
            return;
        };
        if expected != self.checker.unit_expression(control) || t.active_branch != Some(0) {
            return;
        }
        let Some((identity, boundary)) = &t.active else {
            return;
        };
        let pending = t.pending_jump.take();
        t.active_branch = None;
        let Some(n) = &mut t.normal else {
            return;
        };
        if !result.continues
            && result.owner.is_none()
            && tail.map(|e| self.checker.unit_expression(e)) == Some(expression)
            && pending.is_none()
            && self.scope_depth == boundary + 1
            && self.owner_branch_depth == 1
            && self.loop_boundaries.as_slice() == [*boundary]
            && self.owner_observation.as_ref().is_some_and(|o| {
                o.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(identity, &o.identity)
            })
            && n.conditional_return
                .as_mut()
                .is_some_and(|i| i.finish_arm(state))
        {
            t.branch_completed[0] = true;
        } else {
            n.valid = false;
        }
    }
    pub(in super::super::super) fn conditional_return_implicit(
        &mut self,
        control: ExpressionId,
        after_cleanup: bool,
        state: &ValueState,
    ) {
        let Some(t) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::ConditionalReturn {
            control: expected, ..
        } = t.expected
        else {
            return;
        };
        if expected != self.checker.unit_expression(control)
            || !t.split_entered
            || t.active_branch.is_some()
            || t.pending_jump.is_some()
            || t.branch_completed != [true, false]
            || self.owner_branch_depth != 1
        {
            return;
        }
        let Some((identity, boundary)) = &t.active else {
            return;
        };
        let Some(n) = &mut t.normal else {
            return;
        };
        if self.scope_depth == boundary + 1
            && self.loop_boundaries.as_slice() == [*boundary]
            && self.owner_observation.as_ref().is_some_and(|o| {
                o.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(identity, &o.identity)
            })
            && n.conditional_return
                .as_mut()
                .is_some_and(|i| i.record(if after_cleanup { 3 } else { 2 }, identity, state))
        {
            if after_cleanup {
                t.branch_completed[1] = true;
            }
        } else {
            n.valid = false;
        }
    }
    pub(in super::super::super) fn conditional_return_split_finish(
        &mut self,
        control: ExpressionId,
        then_events: &[UnitOwnerTransportEvent],
        else_events: &[UnitOwnerTransportEvent],
        continuations: usize,
    ) -> bool {
        let Some(t) = self.owner_loop.as_mut() else {
            return false;
        };
        let LoopBodyCompletion::ConditionalReturn {
            control: expected, ..
        } = t.expected
        else {
            return false;
        };
        let Some((identity, _)) = &t.active else {
            return false;
        };
        let Some(n) = &mut t.normal else {
            return false;
        };
        if expected == self.checker.unit_expression(control)
            && t.split_entered
            && t.branch_completed == [true, true]
            && t.pending_jump.is_none()
            && t.active_branch.is_none()
            && t.completed.is_none()
            && else_events.is_empty()
            && continuations == 1
            && n.valid
            && self.owner_observation.as_ref().is_some_and(|o| {
                o.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(identity, &o.identity)
            })
            && n.conditional_return
                .as_mut()
                .is_some_and(|i| i.seal(then_events))
        {
            t.completed = Some((identity.clone(), t.expected));
            true
        } else {
            n.valid = false;
            false
        }
    }
    pub(in super::super::super) fn conditional_return_if_completed(
        &mut self,
        control: ExpressionId,
        continues: bool,
        state: &ValueState,
    ) {
        let Some(t) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::ConditionalReturn {
            control: expected, ..
        } = t.expected
        else {
            return;
        };
        if expected != self.checker.unit_expression(control) {
            return;
        }
        let Some((identity, _)) = &t.active else {
            return;
        };
        let Some(n) = &mut t.normal else {
            return;
        };
        if !continues
            || !t
                .completed
                .as_ref()
                .is_some_and(|(i, c)| std::sync::Arc::ptr_eq(i, identity) && *c == t.expected)
            || !n
                .conditional_return
                .as_mut()
                .is_some_and(|i| i.record(4, identity, state))
        {
            n.valid = false;
        }
    }
}

impl DropPlanner<'_, '_> {
    fn conditional_return_context(&self) -> bool {
        let Some(t) = self.owner_loop.as_ref() else {
            return false;
        };
        let Some((identity, boundary)) = &t.active else {
            return false;
        };
        matches!(t.expected, LoopBodyCompletion::ConditionalReturn { .. })
            && t.split_entered
            && t.active_branch == Some(0)
            && t.pending_jump.is_none()
            && t.completed.is_none()
            && t.normal
                .as_ref()
                .is_some_and(|n| n.valid && n.conditional_return.is_some())
            && self.owner_branch_depth == 1
            && self.scope_depth == boundary + 2
            && self.loop_boundaries.as_slice() == [*boundary]
            && self.owner_observation.as_ref().is_some_and(|o| {
                o.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(identity, &o.identity)
            })
    }
    pub(in super::super::super) fn take_conditional_return(
        &mut self,
        id: ExpressionId,
        value: Option<&OwnedValue>,
    ) {
        if !self.conditional_return_context() {
            return;
        }
        let n = self
            .owner_loop
            .as_mut()
            .and_then(|t| t.normal.as_mut())
            .expect("checked Return context");
        n.valid &= n
            .conditional_return
            .as_mut()
            .is_some_and(|i| i.take(self.checker.unit_expression(id), value));
    }
    pub(in super::super::super) fn deliver_conditional_return(
        &mut self,
        id: ExpressionId,
        payload: ExpressionId,
        owner: Option<&UnitOwnerId>,
        state: &ValueState,
    ) {
        if !self.conditional_return_context() {
            return;
        }
        let n = self
            .owner_loop
            .as_mut()
            .and_then(|t| t.normal.as_mut())
            .expect("checked Return context");
        n.valid &= n.conditional_return.as_mut().is_some_and(|i| {
            i.deliver(
                self.checker.unit_expression(id),
                self.checker.unit_expression(payload),
                owner,
                state,
            )
        });
    }
    pub(in super::super::super) fn finish_conditional_return(
        &mut self,
        id: ExpressionId,
        state: &ValueState,
    ) {
        if !self.conditional_return_context() {
            return;
        }
        let n = self
            .owner_loop
            .as_mut()
            .and_then(|t| t.normal.as_mut())
            .expect("checked Return context");
        n.valid &= n
            .conditional_return
            .as_mut()
            .is_some_and(|i| i.cleanup(self.checker.unit_expression(id), state));
    }
    pub(in super::super::super) fn conditional_return_cleaned(&self, id: ExpressionId) -> bool {
        self.conditional_return_context()
            && self
                .owner_loop
                .as_ref()
                .and_then(|t| t.normal.as_ref())
                .and_then(|n| n.conditional_return.as_ref())
                .is_some_and(|i| i.cleaned(self.checker.unit_expression(id)))
    }
}
