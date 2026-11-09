//! Observe each terminated arm before the existing If merge discards its state.
use super::*;
use crate::ownership_checking::{
    UnitOwnerContinueArmBoundaryV1, UnitOwnerInvariantSplitContinueObservationV1,
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
    jump: Option<(UnitExpressionId, UnitOwnerLoopBindingV1)>,
    completion: Option<UnitOwnerLoopBindingV1>,
}
#[derive(Default)]
pub(super) struct InvariantContinueSplitInput {
    entry: Option<(std::sync::Arc<()>, UnitOwnerLoopBindingV1)>,
    arms: [ArmInput; 2],
}
impl ArmInput {
    fn finish(self, branch: usize) -> Option<UnitOwnerContinueArmBoundaryV1> {
        let (identity, control, body, entry) = self.entry?;
        let (jump, after_continue) = self.jump?;
        Some(UnitOwnerContinueArmBoundaryV1 {
            identity,
            control,
            branch,
            body,
            jump,
            entry,
            after_continue,
            completion: self.completion?,
        })
    }
}
impl InvariantContinueSplitInput {
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
    ) -> Option<UnitOwnerInvariantSplitContinueObservationV1> {
        let LoopBodyCompletion::Conditional {
            control, condition, ..
        } = closed.completion
        else {
            return None;
        };
        let (split_identity, split) = self.entry?;
        let [left, right] = self.arms;
        Some(UnitOwnerInvariantSplitContinueObservationV1 {
            closed,
            control,
            branch_condition: condition,
            inputs,
            entry,
            split_identity,
            split,
            arms: [left.finish(0)?, right.finish(1)?],
            exit,
        })
    }
}
impl InvariantInput {
    pub(in super::super) fn enable_split_continue(&mut self) {
        self.split = Some(InvariantContinueSplitInput::default());
    }
    pub(super) fn split_jump_boundary(
        &mut self,
        branch: usize,
        jump: UnitExpressionId,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
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
            arm.jump = Some((jump, binding));
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
                .is_some_and(InvariantContinueSplitInput::is_complete)
        })
    }
}
