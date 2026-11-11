//! Observe actual dynamic-tree boundaries without changing legacy cleanup.
#[path = "multi_continuation_hooks.rs"]
mod multi_continuation_hooks;
#[path = "multi_continuation_plan.rs"]
pub(super) mod multi_continuation_plan;
#[path = "single_continuation_hooks.rs"]
mod single_continuation_hooks;
#[path = "single_continuation_plan.rs"]
pub(super) mod single_continuation_plan;
use super::bounded_forward_plan::Kind;

#[derive(Clone, Copy)]
pub(super) enum BoundedCoverage {
    Terminal(action_source::BoundedState),
    Continuation(action_source::SingleState),
    MultiContinuation(action_source::MultiState),
}
use super::*;
impl DropPlanner<'_, '_> {
    pub(super) fn bounded_boundary_clear(
        &self,
        state: &ValueState,
        expected: Option<action_source::BoundedState>,
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
            matches!(borrow_forward_transports.as_slice(), [pending] if expected.is_some_and(|s| pending.matches_bounded(s)))
        } else {
            borrow_forward_transports.is_empty()
        };
        multi_continuation.is_none()
            && multi_merge.is_none()
            && single_continuation.is_none()
            && *bounded_forward == expected
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

    pub(super) fn begin_bounded_coverage(
        &mut self,
        item: ItemId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Option<BoundedCoverage> {
        if state.nested_forward.is_some()
            || state.branch_forward_plan.is_some()
            || state.forward_scope.is_some()
        {
            return None;
        }
        let Some(plan) = self.bounded_forward_plan(item, body) else {
            if let Some(scope) = self.begin_single_coverage(item, body, state) {
                return Some(BoundedCoverage::Continuation(scope));
            }
            return self
                .begin_multi_coverage(item, body, state)
                .map(BoundedCoverage::MultiContinuation);
        };
        let base = (self.scope_depth, self.owner_branch_depth);
        let clear = self.bounded_boundary_clear(state, None, false, base.0, base.1);
        let scope = self.iteration_actions.begin_bounded(plan, base, clear);
        state.bounded_forward = Some(scope);
        Some(BoundedCoverage::Terminal(scope))
    }
    pub(super) fn observe_bounded_condition(
        &mut self,
        id: ExpressionId,
        condition: ExpressionId,
        state: &ValueState,
        after: bool,
    ) {
        self.observe_single_condition(id, condition, state, after);
        self.observe_multi_condition(id, condition, state, after);
        let Some(scope) = state.bounded_forward else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.bounded_node(scope) else {
            return;
        };
        let clear = matches!(node.kind,Kind::Split{flag,..} if self.checker.place(condition).ok().flatten().is_some_and(|p|p.is_root()&&p.root()==flag))
            && self.checker.ordinary_condition_outcomes(condition) == (true, true)
            && self.bounded_boundary_clear(
                state,
                Some(scope),
                false,
                base.0 + 1 + node.depth,
                base.1 + node.depth,
            );
        self.iteration_actions.bounded_condition(
            scope,
            self.checker.unit_expression(id),
            self.checker.unit_expression(condition),
            after,
            clear,
        );
    }
    pub(super) fn enter_bounded_arm(
        &mut self,
        id: ExpressionId,
        side: bool,
        state: &mut ValueState,
    ) {
        self.enter_single_arm(id, side, state);
        self.enter_multi_arm(id, side, state);
        let Some(scope) = state.bounded_forward else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.bounded_node(scope) else {
            state.bounded_forward = None;
            return;
        };
        let clear = self.bounded_boundary_clear(
            state,
            Some(scope),
            false,
            base.0 + 1 + node.depth,
            base.1 + 1 + node.depth,
        );
        state.bounded_forward = self.iteration_actions.bounded_arm(
            scope,
            self.checker.unit_expression(id),
            side,
            clear,
        );
    }
    pub(super) fn observe_bounded_return(
        &mut self,
        id: ExpressionId,
        state: &ValueState,
        post: bool,
    ) {
        self.observe_single_return(id, state, post);
        self.observe_multi_return(id, state, post);
        let Some(scope) = state.bounded_forward else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.bounded_node(scope) else {
            return;
        };
        let clear = self.bounded_boundary_clear(
            state,
            Some(scope),
            !post,
            base.0 + 1 + node.depth,
            base.1 + node.depth,
        );
        self.iteration_actions
            .bounded_return(scope, self.checker.unit_expression(id), post, clear);
    }
    pub(super) fn observe_bounded_join(
        &mut self,
        scope: Option<action_source::BoundedState>,
        id: ExpressionId,
        then_terminated: bool,
        else_terminated: bool,
        states: &[ValueState],
        state: &ValueState,
    ) {
        let Some(scope) = scope else {
            return;
        };
        let Some((node, base)) = self.iteration_actions.bounded_node(scope) else {
            return;
        };
        let clear = self.bounded_boundary_clear(
            state,
            Some(scope),
            false,
            base.0 + 1 + node.depth,
            base.1 + 1 + node.depth,
        );
        self.iteration_actions.bounded_join(
            scope,
            self.checker.unit_expression(id),
            then_terminated && else_terminated,
            states.is_empty(),
            clear,
        );
    }
    pub(super) fn finish_bounded_coverage(
        &mut self,
        scope: Option<BoundedCoverage>,
        continues: bool,
        state: &ValueState,
    ) {
        let scope = match scope {
            Some(BoundedCoverage::Terminal(scope)) => scope,
            Some(BoundedCoverage::Continuation(scope)) => {
                self.finish_single_coverage(scope, continues, state);
                return;
            }
            Some(BoundedCoverage::MultiContinuation(scope)) => {
                self.finish_multi_coverage(scope, continues, state);
                return;
            }
            None => return,
        };
        let Some((_, base)) = self.iteration_actions.bounded_node(scope) else {
            return;
        };
        let clear = self.bounded_boundary_clear(state, None, false, base.0, base.1);
        self.iteration_actions
            .seal_bounded(scope, !continues, clear);
    }
}
