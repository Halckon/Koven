//! Narrow branch state witnesses; the expression-region boundary is deliberately unchanged.
use super::*;
use super::{borrow_forward_transport::ForwardScope, branch_forward_scope::BranchForwardPlan};
use crate::ownership_checking::UnitBorrowBranchForwardTopology;

impl DropPlanner<'_, '_> {
    pub(super) fn branch_boundary_clear(
        &self,
        state: &ValueState,
        scope: Option<ForwardScope>,
        plan: Option<BranchForwardPlan>,
        live: bool,
        depth: usize,
        owner_depth: usize,
    ) -> bool {
        let ValueState {
            forward_scope,
            branch_forward_plan,
            nested_forward,
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
            matches!(borrow_forward_transports.as_slice(), [pending] if scope.is_some_and(|s| pending.matches_region(s)))
        } else {
            borrow_forward_transports.is_empty()
        };
        nested_forward.is_none()
            && self.scope_depth == depth
            && self.owner_branch_depth == owner_depth
            && *forward_scope == scope
            && *branch_forward_plan == plan
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

    pub(super) fn begin_branch_coverage(&mut self, state: &mut ValueState) -> Option<usize> {
        let mut plan = state.branch_forward_plan?;
        let clear = self.branch_boundary_clear(
            state,
            None,
            Some(plan),
            false,
            self.scope_depth,
            self.owner_branch_depth,
        );
        let id = self.iteration_actions.begin_branch_region(
            plan.scopes,
            plan.topology,
            (self.scope_depth, self.owner_branch_depth),
            clear,
        );
        plan.coverage = Some(id);
        state.branch_forward_plan = Some(plan);
        Some(id)
    }

    pub(super) fn observe_branch_condition(
        &mut self,
        id: ExpressionId,
        condition: ExpressionId,
        state: &ValueState,
        after: bool,
    ) {
        let Some(plan) = state.branch_forward_plan else {
            return;
        };
        let Some(candidate) = plan.coverage else {
            return;
        };
        let Some((depth, owner)) = self.iteration_actions.branch_base(candidate) else {
            return;
        };
        let clear = plan.split == self.checker.unit_expression(id)
            && plan.condition == self.checker.unit_expression(condition)
            && self
                .checker
                .place(condition)
                .ok()
                .flatten()
                .is_some_and(|p| p.is_root() && p.root() == plan.flag)
            && self.branch_boundary_clear(state, None, Some(plan), false, depth + 1, owner);
        self.iteration_actions
            .branch_condition(candidate, after, clear);
    }

    pub(super) fn observe_branch_return(&mut self, state: &ValueState, post: bool) {
        let Some(scope) = state.forward_scope else {
            return;
        };
        let Some(branch) = scope.branch else { return };
        let Some(candidate) = self.iteration_actions.branch_region_for_scope(scope) else {
            return;
        };
        let Some((depth, owner)) = self.iteration_actions.branch_base(candidate) else {
            return;
        };
        let true_arm = match self.iteration_actions.branch_topology(candidate) {
            Some(UnitBorrowBranchForwardTopology::TailContinuation) => {
                usize::from(branch.then_arm())
            }
            Some(UnitBorrowBranchForwardTopology::BothArmsReturn) => 1,
            None => return,
        };
        let clear = self.branch_boundary_clear(
            state,
            Some(scope),
            None,
            !post,
            depth + 1 + true_arm,
            owner + true_arm,
        );
        self.iteration_actions
            .branch_return(candidate, scope, post, clear);
    }

    pub(super) fn observe_branch_join(
        &mut self,
        plan: Option<BranchForwardPlan>,
        terminated: bool,
        else_terminated: bool,
        states: &[ValueState],
    ) {
        let Some(candidate) = plan.and_then(|p| p.coverage) else {
            return;
        };
        if plan.is_some_and(|p| p.topology == UnitBorrowBranchForwardTopology::BothArmsReturn) {
            self.iteration_actions.branch_terminal_join(
                candidate,
                terminated && else_terminated,
                states.is_empty(),
            );
            return;
        }
        let Some((depth, owner)) = self.iteration_actions.branch_base(candidate) else {
            return;
        };
        let clear = states.len() == 1
            && states.iter().all(|state| {
                self.branch_boundary_clear(
                    state,
                    state.forward_scope,
                    None,
                    false,
                    depth + 1,
                    owner + 1,
                )
            });
        let scopes = states.iter().map(|s| s.forward_scope).collect::<Vec<_>>();
        self.iteration_actions
            .branch_continuation(candidate, terminated, &scopes, clear);
    }

    pub(super) fn finish_branch_coverage(
        &mut self,
        candidate: Option<usize>,
        continues: bool,
        state: &ValueState,
    ) {
        let Some(id) = candidate else { return };
        let Some((depth, owner)) = self.iteration_actions.branch_base(id) else {
            return;
        };
        let clear = self.branch_boundary_clear(state, None, None, false, depth, owner);
        self.iteration_actions
            .seal_branch_region(id, !continues, clear);
    }
}
