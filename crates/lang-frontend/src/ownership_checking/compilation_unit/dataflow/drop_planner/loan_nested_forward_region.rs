//! Positive boundaries for the fixed three- or four-leaf terminal certificate.
use super::nested_forward_scope::NestedForwardState;
use super::*;

impl DropPlanner<'_, '_> {
    pub(super) fn nested_boundary_clear(
        &self,
        state: &ValueState,
        expected: Option<NestedForwardState>,
        live: bool,
        depth: usize,
        owner_depth: usize,
    ) -> bool {
        let ValueState {
            forward_scope,
            branch_forward_plan,
            nested_forward,
            bounded_forward,
            single_continuation,
            multi_continuation,
            multi_merge,
            replace_consumer,
            lambda,
            borrow_bindings,
            pending_borrow_results,
            borrow_initializer_sources,
            pending_loans,
            borrow_call_transports,
            borrow_binding_transports,
            borrow_forward_transports,
            iterations,
            pending_temporaries,
            values,
            replacements,
            pending_borrows,
            pending_capture_borrows,
            closures,
            closure_asap,
            this,
        } = state;
        let transport = if live {
            matches!(borrow_forward_transports.as_slice(), [pending] if expected.is_some_and(|s| pending.matches_nested(s)))
        } else {
            borrow_forward_transports.is_empty()
        };
        multi_continuation.is_none()
            && multi_merge.is_none()
            && single_continuation.is_none()
            && bounded_forward.is_none()
            && *nested_forward == expected
            && self.scope_depth == depth
            && self.owner_branch_depth == owner_depth
            && forward_scope.is_none()
            && branch_forward_plan.is_none()
            && self.loop_boundaries.is_empty()
            && self.owner_choice.is_none()
            && self.owner_loop.is_none()
            && self.call_return_copy_source.is_none()
            && self.pending_borrow_handoffs.is_empty()
            && replace_consumer.is_none()
            && lambda.is_none()
            && this.is_none()
            && borrow_bindings.is_empty()
            && pending_borrow_results.is_empty()
            && borrow_initializer_sources.is_empty()
            && pending_loans.is_empty()
            && borrow_call_transports.is_empty()
            && borrow_binding_transports.is_empty()
            && transport
            && iterations.is_empty()
            && pending_temporaries.is_empty()
            && values.is_empty()
            && replacements.is_empty()
            && pending_borrows.is_empty()
            && pending_capture_borrows.is_empty()
            && closures.is_empty()
            && closure_asap.is_empty()
    }

    pub(super) fn begin_nested_coverage(&mut self, state: &mut ValueState) -> Option<usize> {
        let mut nested = state.nested_forward?;
        let base = (self.scope_depth, self.owner_branch_depth);
        let clear = self.nested_boundary_clear(state, Some(nested), false, base.0, base.1);
        let id = self
            .iteration_actions
            .begin_nested_region(nested.plan, base, clear);
        nested.coverage = Some(id);
        state.nested_forward = Some(nested);
        Some(id)
    }

    pub(super) fn observe_nested_condition(
        &mut self,
        id: ExpressionId,
        condition: ExpressionId,
        state: &ValueState,
        after: bool,
    ) {
        let Some(nested) = state.nested_forward else {
            return;
        };
        let Some(candidate) = nested.coverage else {
            return;
        };
        let Some((depth, owner)) = self.iteration_actions.nested_region_base(candidate) else {
            return;
        };
        let Some(index) = nested
            .plan
            .splits()
            .iter()
            .position(|split| split.split == self.checker.unit_expression(id))
        else {
            return;
        };
        let Some(nesting) = nested.plan.split_depth(index) else {
            return;
        };
        let split = nested.plan.splits()[index];
        let clear = split.split == self.checker.unit_expression(id)
            && split.condition == self.checker.unit_expression(condition)
            && nested.ready_split.is_none()
            && nested.leaf.is_none()
            && self
                .checker
                .place(condition)
                .ok()
                .flatten()
                .is_some_and(|p| p.is_root() && p.root() == split.flag)
            && self.nested_boundary_clear(
                state,
                Some(nested),
                false,
                depth + 1 + nesting,
                owner + nesting,
            );
        self.iteration_actions
            .nested_region_condition(candidate, index, after, clear);
    }

    pub(super) fn observe_nested_arm(
        &mut self,
        state: &ValueState,
        index: usize,
        then_arm: bool,
        entry: usize,
    ) {
        let Some(nested) = state.nested_forward else {
            return;
        };
        let Some(candidate) = nested.coverage else {
            return;
        };
        let Some((depth, owner)) = self.iteration_actions.nested_region_base(candidate) else {
            return;
        };
        let Some(nesting) = nested.plan.split_depth(index) else {
            return;
        };
        let clear = nested.path[nesting] == Some(entry)
            && nested.ready_split.is_none()
            && self.nested_boundary_clear(
                state,
                Some(nested),
                false,
                depth + 1 + nesting,
                owner + 1 + nesting,
            );
        self.iteration_actions
            .nested_region_arm(candidate, index, then_arm, entry, clear);
    }

    pub(super) fn observe_nested_return(
        &mut self,
        id: ExpressionId,
        state: &ValueState,
        post: bool,
    ) {
        let Some(nested) = state.nested_forward else {
            return;
        };
        let Some(candidate) = nested.coverage else {
            return;
        };
        let Some(leaf) = nested.leaf else {
            return;
        };
        let Some((depth, owner)) = self.iteration_actions.nested_region_base(candidate) else {
            return;
        };
        let nesting =
            if matches!(nested.plan.topology,
            crate::ownership_checking::UnitBorrowNestedForwardTopology::NestedThenTail
            | crate::ownership_checking::UnitBorrowNestedForwardTopology::NestedElseCascadeTail)
                && leaf == 2
            {
                0
            } else {
                nested.plan.leaves()[leaf].path.iter().flatten().count()
            };
        let clear = nested.plan.leaves()[leaf].control == self.checker.unit_expression(id)
            && nested.ready_split.is_none()
            && self.nested_boundary_clear(
                state,
                Some(nested),
                !post,
                depth + 1 + nesting,
                owner + nesting,
            );
        self.iteration_actions
            .nested_region_return(candidate, leaf, post, clear);
    }

    pub(super) fn observe_nested_join(
        &mut self,
        nested: Option<NestedForwardState>,
        id: ExpressionId,
        then_terminated: bool,
        else_terminated: bool,
        states: &[ValueState],
        state: &ValueState,
    ) -> Option<action_source::ContinuationToken> {
        let nested = nested?;
        let candidate = nested.coverage?;
        let index = nested.ready_split?;
        let nesting = nested.plan.split_depth(index)?;
        let (depth, owner) = self.iteration_actions.nested_region_base(candidate)?;
        let clear = nested.plan.splits()[index].split == self.checker.unit_expression(id)
            && self.nested_boundary_clear(
                state,
                Some(nested),
                false,
                depth + 1 + nesting,
                owner + 1 + nesting,
            );
        if nested.plan.topology
            == crate::ownership_checking::UnitBorrowNestedForwardTopology::NestedElseCascadeTail
        {
            let successor = match states {
                [survivor] => survivor.nested_forward.filter(|next| {
                    self.nested_boundary_clear(
                        survivor,
                        Some(*next),
                        false,
                        depth + 1 + nesting,
                        owner + 1 + nesting,
                    )
                }),
                _ => None,
            };
            return self.iteration_actions.nested_cascade_continuation(
                candidate,
                index,
                successor,
                then_terminated,
                else_terminated,
                clear && states.len() == 1,
            );
        }
        if nested.plan.topology
            == crate::ownership_checking::UnitBorrowNestedForwardTopology::NestedThenTail
            && index == 0
        {
            let successor = match states {
                [survivor] => survivor.nested_forward.filter(|next| {
                    self.nested_boundary_clear(survivor, Some(*next), false, depth + 1, owner + 1)
                }),
                _ => None,
            };
            return self.iteration_actions.nested_region_continuation(
                candidate,
                successor,
                then_terminated,
                else_terminated,
                clear && states.len() == 1,
            );
        }
        self.iteration_actions.nested_region_join(
            candidate,
            index,
            then_terminated && else_terminated,
            states.is_empty(),
            clear,
        );
        None
    }

    pub(super) fn resume_nested_join(
        &mut self,
        token: Option<action_source::ContinuationToken>,
        state: &mut ValueState,
    ) {
        let Some(token) = token else {
            return;
        };
        let clear = self
            .iteration_actions
            .nested_continuation_base(&token)
            .is_some_and(|(depth, owner)| {
                self.nested_boundary_clear(state, None, false, depth + 1, owner + 1)
            });
        // The ordinary merge cleared nested metadata. Only the actual consumed edge token
        // restores the exact pre-merge survivor; no AST-derived state is reconstructed.
        state.nested_forward = self
            .iteration_actions
            .resume_nested_continuation(token, clear);
    }

    pub(super) fn finish_nested_coverage(
        &mut self,
        candidate: Option<usize>,
        continues: bool,
        state: &ValueState,
    ) {
        let Some(id) = candidate else {
            return;
        };
        let Some((depth, owner)) = self.iteration_actions.nested_region_base(id) else {
            return;
        };
        let clear = self.nested_boundary_clear(state, None, false, depth, owner);
        self.iteration_actions
            .seal_nested_region(id, !continues, clear);
    }
}
