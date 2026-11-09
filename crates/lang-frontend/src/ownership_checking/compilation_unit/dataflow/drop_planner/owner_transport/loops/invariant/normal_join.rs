//! Normal arm states and the actual post-merge state are read separately.
use super::*;
use crate::ownership_checking::{
    UnitOwnerInvariantNormalJoinObservationV1, UnitOwnerLoopEdgeV1, UnitOwnerNormalArmBoundaryV1,
};

#[derive(Default)]
struct ArmInput {
    entry: Option<(
        std::sync::Arc<()>,
        UnitExpressionId,
        UnitStatementId,
        UnitOwnerLoopBindingV1,
    )>,
    completion: Option<UnitOwnerLoopBindingV1>,
}
#[derive(Default)]
pub(super) struct InvariantNormalJoinInput {
    split: Option<(std::sync::Arc<()>, UnitOwnerLoopBindingV1)>,
    arms: [ArmInput; 2],
    join: Option<(std::sync::Arc<()>, UnitOwnerLoopBindingV1)>,
}
impl ArmInput {
    fn finish(self, branch: usize) -> Option<UnitOwnerNormalArmBoundaryV1> {
        let (identity, control, body, entry) = self.entry?;
        Some(UnitOwnerNormalArmBoundaryV1 {
            identity,
            control,
            branch,
            body,
            edge: UnitOwnerLoopEdgeV1::IfFallthrough(body, control),
            entry,
            completion: self.completion?,
        })
    }
}
impl InvariantNormalJoinInput {
    pub(super) fn arms_complete(&self) -> bool {
        self.split.is_some()
            && self
                .arms
                .iter()
                .all(|arm| arm.entry.is_some() && arm.completion.is_some())
    }
    pub(super) fn is_complete(&self) -> bool {
        self.arms_complete() && self.join.is_some()
    }
    pub(super) fn finish(
        self,
        closed: UnitOwnerLoopObservationV1,
        inputs: UnitOwnerProducerCatalogV1,
        entry: UnitOwnerLoopBindingV1,
        backedge: UnitOwnerLoopBindingV1,
        exit: UnitOwnerLoopBindingV1,
    ) -> Option<InvariantLoopValueV1> {
        let LoopBodyCompletion::Conditional {
            control,
            condition,
            branches,
        } = closed.completion
        else {
            return None;
        };
        if !branches
            .iter()
            .all(|branch| branch.exit == LoopBranchExit::Fallthrough)
        {
            return None;
        }
        let (split_identity, split) = self.split?;
        let (join_identity, join) = self.join?;
        let [left, right] = self.arms;
        let value = UnitOwnerInvariantNormalJoinObservationV1 {
            closed,
            control,
            branch_condition: condition,
            inputs,
            entry,
            split_identity,
            split,
            arms: [left.finish(0)?, right.finish(1)?],
            join_identity,
            join,
            backedge,
            exit,
        };
        value
            .is_consistent()
            .then_some(InvariantLoopValueV1::NormalJoin(Box::new(value)))
    }
    pub(super) fn enter(
        &mut self,
        identity: std::sync::Arc<()>,
        binding: UnitOwnerLoopBindingV1,
    ) -> bool {
        if self.split.is_some() {
            return false;
        }
        self.split = Some((identity, binding));
        true
    }
    pub(super) fn arm_entry(
        &mut self,
        branch: usize,
        identity: std::sync::Arc<()>,
        control: UnitExpressionId,
        body: UnitStatementId,
        binding: UnitOwnerLoopBindingV1,
    ) -> bool {
        let Some(arm) = self.arms.get_mut(branch) else {
            return false;
        };
        if self.split.is_none()
            || arm.entry.is_some()
            || arm.completion.is_some()
            || self.join.is_some()
        {
            return false;
        }
        arm.entry = Some((identity, control, body, binding));
        true
    }
    pub(super) fn arm_completion(
        &mut self,
        branch: usize,
        binding: UnitOwnerLoopBindingV1,
    ) -> bool {
        let Some(arm) = self.arms.get_mut(branch) else {
            return false;
        };
        if arm.entry.is_none() || arm.completion.is_some() || self.join.is_some() {
            return false;
        }
        arm.completion = Some(binding);
        true
    }
}
impl DropPlanner<'_, '_> {
    // Called after merge_resource_states, never from a pre-merge arm or expected AST edge.
    pub(in super::super::super) fn finish_owner_loop_join(
        &mut self,
        control: ExpressionId,
        state: &ValueState,
    ) {
        let Some(transaction) = &mut self.owner_loop else {
            return;
        };
        let LoopBodyCompletion::Conditional {
            control: expected,
            branches,
            ..
        } = transaction.expected
        else {
            return;
        };
        let Some((identity, boundary)) = &transaction.active else {
            return;
        };
        if expected != self.checker.unit_expression(control)
            || !branches
                .iter()
                .all(|branch| branch.exit == LoopBranchExit::Fallthrough)
            || transaction.branch_completed != [true, true]
            || !transaction.split_entered
            || transaction.active_branch.is_some()
            || transaction.pending_jump.is_some()
            || !transaction
                .completed
                .as_ref()
                .is_some_and(|(completed, kind)| {
                    std::sync::Arc::ptr_eq(completed, identity) && *kind == transaction.expected
                })
            || self.scope_depth != boundary + 1
            || self.owner_branch_depth != 1
            || self.loop_boundaries.as_slice() != [*boundary]
            || !self.owner_observation.as_ref().is_some_and(|observation| {
                observation.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(&observation.identity, identity)
            })
        {
            return;
        }
        let Some(input) = &mut transaction.invariant else {
            return;
        };
        let Some(binding) = input.snapshot(state) else {
            return;
        };
        let Some(normal) = &mut input.normal else {
            return;
        };
        if normal.arms_complete() && normal.join.is_none() {
            normal.join = Some((identity.clone(), binding));
        }
    }
}

impl InvariantInput {
    pub(in super::super) fn enable_normal_join(&mut self) {
        self.normal = Some(InvariantNormalJoinInput::default());
    }
}
