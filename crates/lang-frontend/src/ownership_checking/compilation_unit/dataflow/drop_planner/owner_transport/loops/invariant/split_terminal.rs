//! Observe each terminated arm before the existing If merge discards its state.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerContinueArmBoundaryV1, UnitOwnerInvariantSplitContinueObservationV1,
    UnitOwnerInvariantSplitExitObservationV1, UnitOwnerJumpArmBoundaryV1, UnitOwnerLoopEdgeV1,
};

type ArmEntry = (
    std::sync::Arc<()>,
    UnitExpressionId,
    UnitStatementId,
    UnitOwnerLoopBindingV1,
);
#[derive(Default)]
struct ArmInput {
    entry: Option<ArmEntry>,
    jump: Option<(LoopBranchExit, UnitOwnerLoopBindingV1)>,
    completion: Option<UnitOwnerLoopBindingV1>,
}
#[derive(Default)]
pub(super) struct InvariantTerminalSplitInput {
    entry: Option<(std::sync::Arc<()>, UnitOwnerLoopBindingV1)>,
    arms: [ArmInput; 2],
}
impl ArmInput {
    fn finish(
        self,
        branch: usize,
        statement: UnitStatementId,
        condition: UnitExpressionId,
    ) -> Option<UnitOwnerJumpArmBoundaryV1> {
        let (identity, control, body, entry) = self.entry?;
        let (actual, after_jump) = self.jump?;
        let (jump, edge) = match actual {
            LoopBranchExit::Continue(jump) => (
                jump,
                UnitOwnerLoopEdgeV1::ContinueBackedge(
                    UnitDropPoint::ControlTransfer(jump),
                    condition,
                ),
            ),
            LoopBranchExit::Break(jump) => (
                jump,
                UnitOwnerLoopEdgeV1::BreakExit(
                    UnitDropPoint::ControlTransfer(jump),
                    UnitDropPoint::LoopExit(statement),
                ),
            ),
            LoopBranchExit::Fallthrough => return None,
        };
        Some(UnitOwnerJumpArmBoundaryV1 {
            identity,
            control,
            branch,
            body,
            jump,
            edge,
            entry,
            after_jump,
            completion: self.completion?,
        })
    }
}
// A public Continue-only arm is constructed only from an actually observed Continue.
fn into_continue(
    arm: UnitOwnerJumpArmBoundaryV1,
    condition: UnitExpressionId,
) -> Option<UnitOwnerContinueArmBoundaryV1> {
    (arm.edge
        == UnitOwnerLoopEdgeV1::ContinueBackedge(
            UnitDropPoint::ControlTransfer(arm.jump),
            condition,
        ))
    .then_some(UnitOwnerContinueArmBoundaryV1 {
        identity: arm.identity,
        control: arm.control,
        branch: arm.branch,
        body: arm.body,
        jump: arm.jump,
        entry: arm.entry,
        after_continue: arm.after_jump,
        completion: arm.completion,
    })
}
impl InvariantTerminalSplitInput {
    pub(super) fn is_complete(&self) -> bool {
        self.entry.is_some()
            && self
                .arms
                .iter()
                .all(|arm| arm.entry.is_some() && arm.jump.is_some() && arm.completion.is_some())
    }
    pub(super) fn finish(
        self,
        closed: UnitOwnerLoopObservationV1,
        inputs: UnitOwnerProducerCatalogV1,
        entry: UnitOwnerLoopBindingV1,
        exit: UnitOwnerLoopBindingV1,
    ) -> Option<InvariantLoopValueV1> {
        let LoopBodyCompletion::Conditional {
            control, condition, ..
        } = closed.completion
        else {
            return None;
        };
        let (split_identity, split) = self.entry?;
        let [left, right] = self.arms;
        let arms = [
            left.finish(0, closed.statement(), closed.condition())?,
            right.finish(1, closed.statement(), closed.condition())?,
        ];
        if arms
            .iter()
            .all(|arm| matches!(arm.edge, UnitOwnerLoopEdgeV1::ContinueBackedge(..)))
        {
            let [left, right] = arms;
            let arms = [
                into_continue(left, closed.condition())?,
                into_continue(right, closed.condition())?,
            ];
            let value = UnitOwnerInvariantSplitContinueObservationV1 {
                closed,
                control,
                branch_condition: condition,
                inputs,
                entry,
                split_identity,
                split,
                arms,
                exit,
            };
            value
                .is_consistent()
                .then_some(InvariantLoopValueV1::SplitContinue(Box::new(value)))
        } else {
            let value = UnitOwnerInvariantSplitExitObservationV1 {
                closed,
                control,
                branch_condition: condition,
                inputs,
                entry,
                split_identity,
                split,
                arms,
                exit,
            };
            value
                .is_consistent()
                .then_some(InvariantLoopValueV1::SplitExit(Box::new(value)))
        }
    }
}
impl InvariantInput {
    pub(in super::super) fn enable_terminal_split(&mut self) {
        self.split = Some(InvariantTerminalSplitInput::default());
    }
    pub(super) fn split_jump_boundary(
        &mut self,
        branch: usize,
        completion: LoopBodyCompletion,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        let actual = match completion {
            LoopBodyCompletion::Continue(jump) => LoopBranchExit::Continue(jump),
            LoopBodyCompletion::Break(jump) => LoopBranchExit::Break(jump),
            _ => return false,
        };
        let Some(binding) = self.snapshot(state) else {
            return false;
        };
        let Some(split) = &mut self.split else {
            return false;
        };
        let Some(arm) = split.arms.get_mut(branch) else {
            return false;
        };
        if arm.jump.is_none()
            && arm.completion.is_none()
            && arm
                .entry
                .as_ref()
                .is_some_and(|(entered, ..)| std::sync::Arc::ptr_eq(entered, identity))
        {
            arm.jump = Some((actual, binding));
            true
        } else {
            false
        }
    }
}
impl LoopTransaction {
    pub(in super::super) fn split_input_boundary(&mut self, state: &ValueState) -> bool {
        let Some(input) = &mut self.invariant else {
            return empty_boundary(state);
        };
        let Some(binding) = input.snapshot(state) else {
            return false;
        };
        let (Some(split), Some((identity, _))) = (&mut input.split, &self.active) else {
            return false;
        };
        if split.entry.is_none() {
            split.entry = Some((identity.clone(), binding));
            true
        } else {
            false
        }
    }
    pub(in super::super) fn split_arm_entry_boundary(
        &mut self,
        branch: usize,
        control: UnitExpressionId,
        body: UnitStatementId,
        state: &ValueState,
    ) -> bool {
        let Some(input) = &mut self.invariant else {
            return empty_boundary(state);
        };
        let Some(binding) = input.snapshot(state) else {
            return false;
        };
        let (Some(split), Some((identity, _))) = (&mut input.split, &self.active) else {
            return false;
        };
        let Some(arm) = split.arms.get_mut(branch) else {
            return false;
        };
        if split.entry.is_some()
            && arm.entry.is_none()
            && arm.jump.is_none()
            && arm.completion.is_none()
        {
            arm.entry = Some((identity.clone(), control, body, binding));
            true
        } else {
            false
        }
    }
    pub(in super::super) fn split_arm_completion_boundary(
        &mut self,
        branch: usize,
        state: &ValueState,
    ) -> bool {
        let Some(input) = &mut self.invariant else {
            return empty_boundary(state);
        };
        let Some(binding) = input.snapshot(state) else {
            return false;
        };
        let Some(arm) = input
            .split
            .as_mut()
            .and_then(|split| split.arms.get_mut(branch))
        else {
            return false;
        };
        if arm.entry.is_some() && arm.jump.is_some() && arm.completion.is_none() {
            arm.completion = Some(binding);
            true
        } else {
            false
        }
    }
    pub(in super::super) fn split_inputs_complete(&self) -> bool {
        self.invariant.as_ref().is_none_or(|input| {
            input
                .split
                .as_ref()
                .is_some_and(InvariantTerminalSplitInput::is_complete)
        })
    }
}
