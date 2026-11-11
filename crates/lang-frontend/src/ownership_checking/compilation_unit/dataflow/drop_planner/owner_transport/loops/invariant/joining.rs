//! Observe real arm completion and only the actual surviving states at the If join.
use super::*;
use crate::ownership_checking::{
    UnitOwnerInvariantMixedJoinObservationV1, UnitOwnerInvariantNormalJoinObservationV1,
    UnitOwnerJumpArmBoundaryV1, UnitOwnerLoopEdgeV1, UnitOwnerMixedArmBoundaryV1,
    UnitOwnerNormalArmBoundaryV1,
};

#[derive(Default)]
struct ArmInput {
    entry: Option<(
        std::sync::Arc<()>,
        UnitExpressionId,
        UnitStatementId,
        UnitOwnerLoopBindingV1,
    )>,
    jump: Option<(LoopBranchExit, UnitOwnerLoopBindingV1)>,
    completion: Option<(bool, UnitOwnerLoopBindingV1)>,
}
#[derive(Default)]
pub(super) struct InvariantJoinInput {
    split: Option<(std::sync::Arc<()>, UnitOwnerLoopBindingV1)>,
    arms: [ArmInput; 2],
    join: Option<(std::sync::Arc<()>, UnitOwnerLoopBindingV1)>,
}
impl ArmInput {
    fn finish(
        self,
        branch: usize,
        statement: UnitStatementId,
        condition: UnitExpressionId,
    ) -> Option<UnitOwnerMixedArmBoundaryV1> {
        let (identity, control, body, entry) = self.entry?;
        let (continues, completion) = self.completion?;
        match (continues, self.jump) {
            (true, None) => Some(UnitOwnerMixedArmBoundaryV1::Normal(
                UnitOwnerNormalArmBoundaryV1 {
                    identity,
                    control,
                    branch,
                    body,
                    edge: UnitOwnerLoopEdgeV1::IfFallthrough(body, control),
                    entry,
                    completion,
                },
            )),
            (false, Some((kind, after_jump))) => {
                let (jump, edge) = match kind {
                    LoopBranchExit::Continue(jump) => (
                        jump,
                        UnitOwnerLoopEdgeV1::ContinueBackedge(
                            crate::ownership_checking::UnitDropPoint::ControlTransfer(jump),
                            condition,
                        ),
                    ),
                    LoopBranchExit::Break(jump) => (
                        jump,
                        UnitOwnerLoopEdgeV1::BreakExit(
                            crate::ownership_checking::UnitDropPoint::ControlTransfer(jump),
                            crate::ownership_checking::UnitDropPoint::LoopExit(statement),
                        ),
                    ),
                    LoopBranchExit::Fallthrough => return None,
                };
                Some(UnitOwnerMixedArmBoundaryV1::Jump(
                    UnitOwnerJumpArmBoundaryV1 {
                        identity,
                        control,
                        branch,
                        body,
                        jump,
                        edge,
                        entry,
                        after_jump,
                        completion,
                    },
                ))
            }
            _ => None,
        }
    }
}
impl InvariantJoinInput {
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
            .any(|branch| branch.exit == LoopBranchExit::Fallthrough)
        {
            return None;
        }
        let (split_identity, split) = self.split?;
        let (join_identity, join) = self.join?;
        let [left, right] = self.arms;
        let arms = [
            left.finish(0, closed.statement(), closed.condition())?,
            right.finish(1, closed.statement(), closed.condition())?,
        ];
        if let [
            UnitOwnerMixedArmBoundaryV1::Normal(left),
            UnitOwnerMixedArmBoundaryV1::Normal(right),
        ] = arms
        {
            let value = UnitOwnerInvariantNormalJoinObservationV1 {
                closed,
                control,
                branch_condition: condition,
                inputs,
                entry,
                split_identity,
                split,
                arms: [left, right],
                join_identity,
                join,
                backedge,
                exit,
            };
            value
                .is_consistent()
                .then_some(InvariantLoopValueV1::NormalJoin(Box::new(value)))
        } else {
            let value = UnitOwnerInvariantMixedJoinObservationV1 {
                closed,
                control,
                branch_condition: condition,
                inputs,
                entry,
                split_identity,
                split,
                arms,
                join_identity,
                join,
                backedge,
                exit,
            };
            value
                .is_consistent()
                .then_some(InvariantLoopValueV1::MixedJoin(Box::new(value)))
        }
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
            || arm.jump.is_some()
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
        continues: bool,
        binding: UnitOwnerLoopBindingV1,
    ) -> bool {
        let Some(arm) = self.arms.get_mut(branch) else {
            return false;
        };
        if arm.entry.is_none()
            || arm.completion.is_some()
            || self.join.is_some()
            || continues == arm.jump.is_some()
        {
            return false;
        }
        arm.completion = Some((continues, binding));
        true
    }
    pub(super) fn arm_jump(
        &mut self,
        branch: usize,
        actual: LoopBranchExit,
        identity: &std::sync::Arc<()>,
        binding: UnitOwnerLoopBindingV1,
    ) -> bool {
        let Some(arm) = self.arms.get_mut(branch) else {
            return false;
        };
        if actual == LoopBranchExit::Fallthrough
            || arm.jump.is_some()
            || arm.completion.is_some()
            || self.join.is_some()
            || !arm
                .entry
                .as_ref()
                .is_some_and(|(entered, ..)| std::sync::Arc::ptr_eq(entered, identity))
        {
            return false;
        }
        arm.jump = Some((actual, binding));
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
        let Some(transaction) = self.owner_loop.as_mut() else {
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
                .any(|branch| branch.exit == LoopBranchExit::Fallthrough)
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
        let Some(joining) = &mut input.joining else {
            return;
        };
        if joining.arms_complete() && joining.join.is_none() {
            joining.join = Some((identity.clone(), binding));
        }
    }
}

impl InvariantInput {
    pub(in super::super) fn enable_joining(&mut self) {
        self.joining = Some(InvariantJoinInput::default());
    }
}
