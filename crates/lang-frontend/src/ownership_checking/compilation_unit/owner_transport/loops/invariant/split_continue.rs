//! Distinct nonempty arm snapshots; no owner is invented after the empty If merge.
use super::super::{
    LoopBodyCompletion, LoopBranchExit, UnitOwnerLoopEdgeV1, UnitOwnerLoopObservationV1,
};
use super::{
    UnitExpressionId, UnitOwnerLoopBindingV1, UnitOwnerObservation, UnitOwnerProducerCatalogV1,
    UnitOwnerValueObservationV1, UnitStatementId,
};

/// Three independently read states of one actual terminal Continue arm.
#[derive(Clone, Debug)]
pub struct UnitOwnerContinueArmBoundaryV1 {
    pub(in crate::ownership_checking::compilation_unit) identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) control: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) branch: usize,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) jump: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) after_continue: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) completion: UnitOwnerLoopBindingV1,
}
impl UnitOwnerContinueArmBoundaryV1 {
    /// Actual If expression selecting this arm.
    pub fn control(&self) -> UnitExpressionId {
        self.control
    }
    /// Zero for then, one for else.
    pub fn branch(&self) -> usize {
        self.branch
    }
    /// Actual arm ControlBody.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Actual Continue expression, not a synthetic shared endpoint.
    pub fn jump(&self) -> UnitExpressionId {
        self.jump
    }
    /// Read after actual branch-entry cleanup.
    pub fn entry(&self) -> &UnitOwnerLoopBindingV1 {
        &self.entry
    }
    /// Read after actual Continue cleanup.
    pub fn after_continue(&self) -> &UnitOwnerLoopBindingV1 {
        &self.after_continue
    }
    /// Reread after the arm returned false, before its state is discarded.
    pub fn completion(&self) -> &UnitOwnerLoopBindingV1 {
        &self.completion
    }
}

/// Invariant input on two mutually exclusive Continue edges, not an owner phi.
/// Both arms terminate. The checker merges no continuation state, so this API
/// deliberately has no body-completion binding or single backedge binding.
#[derive(Clone, Debug)]
pub struct UnitOwnerInvariantSplitContinueObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) closed: UnitOwnerLoopObservationV1,
    pub(in crate::ownership_checking::compilation_unit) control: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) branch_condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) split_identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) split: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) arms: [UnitOwnerContinueArmBoundaryV1; 2],
    pub(in crate::ownership_checking::compilation_unit) exit: UnitOwnerLoopBindingV1,
}
impl UnitOwnerInvariantSplitContinueObservationV1 {
    /// Actual While statement.
    pub fn statement(&self) -> UnitStatementId {
        self.closed.statement()
    }
    /// Actual While condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.closed.condition()
    }
    /// Actual body that returned false with no continuing If state.
    pub fn body(&self) -> UnitStatementId {
        self.closed.body()
    }
    /// Actual terminal If control.
    pub fn control(&self) -> UnitExpressionId {
        self.control
    }
    /// Actual independent Boolean condition of the If.
    pub fn branch_condition(&self) -> UnitExpressionId {
        self.branch_condition
    }
    /// Real input producers, not final consumption evidence.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.inputs
    }
    /// Read after actual While condition evaluation.
    pub fn entry(&self) -> &UnitOwnerLoopBindingV1 {
        &self.entry
    }
    /// Independently read after actual If condition evaluation.
    pub fn split_boundary(&self) -> &UnitOwnerLoopBindingV1 {
        &self.split
    }
    /// Two ordered, independently traversed arms. Not a finite runtime trace.
    pub fn arms(&self) -> &[UnitOwnerContinueArmBoundaryV1; 2] {
        &self.arms
    }
    /// Read after LoopExit cleanup on the original condition state.
    pub fn exit(&self) -> &UnitOwnerLoopBindingV1 {
        &self.exit
    }
    /// Closed body-local transport, excluding the input owner.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        self.closed.body_transport()
    }
    /// Entry, two If arms and their Continue edges, condition-false exit.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        self.closed.edges()
    }
    /// Identity and real-edge seal, not a join or full owner conservation proof.
    pub fn is_consistent(&self) -> bool {
        let LoopBodyCompletion::Conditional {
            control,
            condition,
            branches,
        } = self.closed.completion
        else {
            return false;
        };
        control == self.control
            && condition == self.branch_condition
            && self.closed.is_consistent()
            && self.inputs.is_consistent()
            && self.inputs.definitions().len() == 1
            && self.inputs.callable() == self.closed.callable
            && std::sync::Arc::ptr_eq(&self.inputs.identity, &self.closed.identity)
            && std::sync::Arc::ptr_eq(&self.split_identity, &self.closed.body_identity)
            && self.entry == self.split
            && self.entry == self.exit
            && self.entry.symbol.source_unit() == self.statement().source_unit()
            && self
                .inputs
                .definition(self.entry.owner())
                .is_some_and(|definition| {
                    matches!(
                        definition.origin(),
                        crate::ownership_checking::UnitOwnerOrigin::String(_)
                    )
                })
            && self
                .arms
                .iter()
                .zip(branches)
                .enumerate()
                .all(|(index, (arm, expected))| {
                    matches!(expected.exit, LoopBranchExit::Continue(jump) if jump == arm.jump)
                        && arm.control == control
                        && arm.branch == index
                        && arm.body == expected.body
                        && std::sync::Arc::ptr_eq(&arm.identity, &self.closed.body_identity)
                        && arm.entry == self.entry
                        && arm.after_continue == self.entry
                        && arm.completion == self.entry
                })
    }
}
impl UnitOwnerObservation {
    /// Distinct nonempty two-Continue-arm capability; old loop APIs remain strict.
    pub fn invariant_split_continue_observation_v1(
        &self,
    ) -> Option<&UnitOwnerInvariantSplitContinueObservationV1> {
        let super::InvariantLoopValueV1::SplitContinue(value) =
            self.invariant_loop_value.as_deref()?
        else {
            return None;
        };
        (self.open_loop_value.is_none()
            && self.loop_value.is_none()
            && matches!(self.status, crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
                point: crate::ownership_checking::UnitDropPoint::AfterStatement(statement),
                reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax
            } if statement == value.statement())
            && value.closed.callable == self.callable
            && std::sync::Arc::ptr_eq(&value.closed.identity, &self.identity)
            && value.is_consistent()).then_some(value)
    }
}
