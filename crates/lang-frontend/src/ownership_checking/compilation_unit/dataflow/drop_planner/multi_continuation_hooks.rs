//! Actual multi-survivor hooks, independent of terminal closure and legacy cleanup.
use super::super::*;
use super::multi_continuation_plan::Kind;
impl DropPlanner<'_, '_> {
    pub(in super::super) fn multi_boundary_clear(
        &self,
        state: &ValueState,
        expected: Option<action_source::MultiState>,
        merged: Option<&[action_source::MultiState]>,
        live: bool,
        depth: usize,
        owner_depth: usize,
    ) -> bool {
        let ValueState {
            forward_scope,
            branch_forward_plan,
            nested_forward,
            bounded_forward,
            multi_continuation,
            multi_merge,
            single_continuation,
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
            matches!(borrow_forward_transports.as_slice(), [pending] if expected.is_some_and(|s| pending.matches_multi(s)))
        } else {
            borrow_forward_transports.is_empty()
        };
        *multi_continuation == expected
            && multi_merge.as_ref().map(|m| m.inputs.as_slice()) == merged
            && single_continuation.is_none()
            && bounded_forward.is_none()
            && nested_forward.is_none()
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

    pub(in super::super) fn begin_multi_coverage(
        &mut self,
        item: ItemId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Option<action_source::MultiState> {
        let plan = self.multi_continuation_plan(item, body)?;
        let base = (self.scope_depth, self.owner_branch_depth);
        let clear = self.multi_boundary_clear(state, None, None, false, base.0, base.1);
        let scope = self.iteration_actions.begin_multi(plan, base, clear);
        state.multi_continuation = Some(scope);
        Some(scope)
    }
    pub(in super::super) fn observe_multi_condition(
        &mut self,
        id: ExpressionId,
        condition: ExpressionId,
        state: &ValueState,
        after: bool,
    ) {
        let Some(scope) = state.multi_continuation else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.multi_node(scope) else {
            return;
        };
        let clear = matches!(node.kind,Kind::Split{flag,..} if self.checker.place(condition).ok().flatten().is_some_and(|p|p.is_root()&&p.root()==flag))
            && self.checker.ordinary_condition_outcomes(condition) == (true, true)
            && self.multi_boundary_clear(
                state,
                Some(scope),
                None,
                false,
                base.0 + 1 + node.depth,
                base.1 + node.depth,
            );
        self.iteration_actions.multi_condition(
            scope,
            self.checker.unit_expression(id),
            self.checker.unit_expression(condition),
            after,
            clear,
        );
    }
    pub(in super::super) fn enter_multi_arm(
        &mut self,
        id: ExpressionId,
        side: bool,
        state: &mut ValueState,
    ) {
        let Some(scope) = state.multi_continuation else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.multi_node(scope) else {
            state.multi_continuation = None;
            return;
        };
        let clear = self.multi_boundary_clear(
            state,
            Some(scope),
            None,
            false,
            base.0 + 1 + node.depth,
            base.1 + 1 + node.depth,
        );
        state.multi_continuation =
            self.iteration_actions
                .multi_arm(scope, self.checker.unit_expression(id), side, clear);
    }
    pub(in super::super) fn observe_multi_hole(&mut self, state: &ValueState) {
        let Some(scope) = state.multi_continuation else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.multi_node(scope) else {
            return;
        };
        let clear = matches!(node.kind, Kind::Hole)
            && node.depth > 0
            && self.multi_boundary_clear(
                state,
                Some(scope),
                None,
                false,
                base.0 + node.depth,
                base.1 + node.depth,
            );
        self.iteration_actions.multi_hole(scope, clear);
    }
    pub(in super::super) fn observe_multi_return(
        &mut self,
        id: ExpressionId,
        state: &ValueState,
        post: bool,
    ) {
        let Some(scope) = state.multi_continuation else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.multi_node(scope) else {
            return;
        };
        let clear = self.multi_boundary_clear(
            state,
            Some(scope),
            None,
            !post,
            base.0 + 1 + node.depth,
            base.1 + node.depth,
        );
        self.iteration_actions
            .multi_return(scope, self.checker.unit_expression(id), post, clear);
    }
    pub(in super::super) fn observe_multi_join(
        &mut self,
        scope: Option<action_source::MultiState>,
        id: ExpressionId,
        then_terminated: bool,
        else_terminated: bool,
        states: &[ValueState],
        state: &ValueState,
    ) -> Option<action_source::MultiResume> {
        let scope = scope?;
        let (node, base) = self.iteration_actions.multi_node(scope)?;
        let clear = self.multi_boundary_clear(
            state,
            Some(scope),
            None,
            false,
            base.0 + 1 + node.depth,
            base.1 + 1 + node.depth,
        );
        let survivors = states
            .iter()
            .map(|only| {
                only.multi_continuation.filter(|next| {
                    self.multi_boundary_clear(
                        only,
                        Some(*next),
                        None,
                        false,
                        base.0 + 1 + node.depth,
                        base.1 + 1 + node.depth,
                    )
                })
            })
            .collect::<Option<Vec<_>>>();
        let cardinality =
            states.len() == usize::from(!then_terminated) + usize::from(!else_terminated);
        self.iteration_actions.multi_join(
            scope,
            self.checker.unit_expression(id),
            then_terminated,
            else_terminated,
            survivors.as_deref().unwrap_or(&[]),
            clear && cardinality && survivors.is_some(),
        )
    }
    pub(in super::super) fn resume_multi_join(
        &mut self,
        token: Option<action_source::MultiResume>,
        state: &mut ValueState,
    ) {
        let Some(token) = token else {
            return;
        };
        let merged = state.multi_merge.as_ref().map(|m| m.inputs.as_slice());
        let clear = self
            .iteration_actions
            .multi_resume_node(&token)
            .is_some_and(|(node, base)| {
                self.multi_boundary_clear(
                    state,
                    None,
                    merged,
                    false,
                    base.0 + 1 + node.depth,
                    base.1 + 1 + node.depth,
                )
            });
        let resumed = self.iteration_actions.multi_resume(token, merged, clear);
        state.multi_merge = None;
        state.multi_continuation = resumed;
    }
    pub(in super::super) fn finish_multi_coverage(
        &mut self,
        scope: action_source::MultiState,
        continues: bool,
        state: &ValueState,
    ) {
        let Some((_, base)) = self.iteration_actions.multi_node(scope) else {
            return;
        };
        let clear = self.multi_boundary_clear(state, None, None, false, base.0, base.1);
        self.iteration_actions.seal_multi(scope, !continues, clear);
    }
}
