//! One real survivor reaches the If join; its sibling remains a distinct jump boundary.
use super::super::{
    LoopBodyCompletion, LoopBranchExit, UnitOwnerLoopEdgeV1, UnitOwnerLoopObservationV1,
};
use super::{
    UnitExpressionId, UnitOwnerLoopBindingV1, UnitOwnerObservation, UnitOwnerProducerCatalogV1,
    UnitOwnerValueObservationV1, UnitStatementId,
};

/// A real normal continuation or terminated sibling; no shared arm state is invented.
#[derive(Clone, Debug)]
pub enum UnitOwnerMixedArmBoundaryV1 {
    /// This empty arm returned true and contributed its state to the actual If merge.
    Normal(super::UnitOwnerNormalArmBoundaryV1),
    /// This jump returned false; its state is observed before being discarded.
    Jump(super::UnitOwnerJumpArmBoundaryV1),
}
impl UnitOwnerMixedArmBoundaryV1 {
    /// Actual If expression.
    pub fn control(&self) -> UnitExpressionId {
        match self {
            Self::Normal(arm) => arm.control(),
            Self::Jump(arm) => arm.control(),
        }
    }
    /// Ordered arm: zero for then, one for else.
    pub fn branch(&self) -> usize {
        match self {
            Self::Normal(arm) => arm.branch(),
            Self::Jump(arm) => arm.branch(),
        }
    }
    /// Actual ControlBody.
    pub fn body(&self) -> UnitStatementId {
        match self {
            Self::Normal(arm) => arm.body(),
            Self::Jump(arm) => arm.body(),
        }
    }
    /// Normal completion or actual jump destination.
    pub fn edge(&self) -> UnitOwnerLoopEdgeV1 {
        match self {
            Self::Normal(arm) => arm.edge(),
            Self::Jump(arm) => arm.edge(),
        }
    }
    /// Independently read after branch-entry cleanup.
    pub fn entry(&self) -> &UnitOwnerLoopBindingV1 {
        match self {
            Self::Normal(arm) => arm.entry(),
            Self::Jump(arm) => arm.entry(),
        }
    }
    /// Independently read after the real arm completion and applicable cleanup.
    pub fn completion(&self) -> &UnitOwnerLoopBindingV1 {
        match self {
            Self::Normal(arm) => arm.completion(),
            Self::Jump(arm) => arm.completion(),
        }
    }
}

/// Unchanged input on one normal empty arm and one Continue/Break arm, plus their real destinations.
/// This proves only identical singleton states, not a general phi or changing owner join.
#[derive(Clone, Debug)]
pub struct UnitOwnerInvariantMixedJoinObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) closed: UnitOwnerLoopObservationV1,
    pub(in crate::ownership_checking::compilation_unit) control: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) branch_condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) split_identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) split: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) arms: [UnitOwnerMixedArmBoundaryV1; 2],
    pub(in crate::ownership_checking::compilation_unit) join_identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) join: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) backedge: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) exit: UnitOwnerLoopBindingV1,
}
impl UnitOwnerInvariantMixedJoinObservationV1 {
    /// Actual While statement.
    pub fn statement(&self) -> UnitStatementId {
        self.closed.statement()
    }
    /// Actual While condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.closed.condition()
    }
    /// Actual body that returned true after its normal scope cleanup.
    pub fn body(&self) -> UnitStatementId {
        self.closed.body()
    }
    /// Actual final If control.
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
    pub fn arms(&self) -> &[UnitOwnerMixedArmBoundaryV1; 2] {
        &self.arms
    }
    /// Independently read after the actual one-survivor If merge; the jump state is absent.
    pub fn join_boundary(&self) -> &UnitOwnerLoopBindingV1 {
        &self.join
    }
    /// Independently read after normal whole-body scope cleanup.
    pub fn backedge(&self) -> &UnitOwnerLoopBindingV1 {
        &self.backedge
    }
    /// Read after LoopExit cleanup on the original condition state.
    pub fn exit(&self) -> &UnitOwnerLoopBindingV1 {
        &self.exit
    }
    /// Closed body-local transport, excluding the input owner.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        self.closed.body_transport()
    }
    /// Entry, both actual arm destinations, one normal body backedge, and condition-false exit.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        self.closed.edges()
    }
    /// Identity and real-join seal, not a general phi or full owner conservation proof.
    pub fn is_consistent(&self) -> bool {
        let LoopBodyCompletion::Conditional {
            control,
            condition,
            branches,
        } = self.closed.completion
        else {
            return false;
        };
        branches
            .iter()
            .filter(|branch| branch.exit == LoopBranchExit::Fallthrough)
            .count()
            == 1
            && control == self.control
            && condition == self.branch_condition
            && self.closed.is_consistent()
            && self.inputs.is_consistent()
            && self.inputs.definitions().len() == 1
            && self.inputs.callable() == self.closed.callable
            && std::sync::Arc::ptr_eq(&self.inputs.identity, &self.closed.identity)
            && std::sync::Arc::ptr_eq(&self.split_identity, &self.closed.body_identity)
            && std::sync::Arc::ptr_eq(&self.join_identity, &self.closed.body_identity)
            && self.entry == self.join
            && self.entry == self.backedge
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
                    arm.control() == control
                        && arm.branch() == index
                        && arm.body() == expected.body
                        && arm.entry() == &self.entry
                        && arm.completion() == &self.entry
                        && match (arm, expected.exit) {
                            (
                                UnitOwnerMixedArmBoundaryV1::Normal(normal),
                                LoopBranchExit::Fallthrough,
                            ) => {
                                std::sync::Arc::ptr_eq(&normal.identity, &self.closed.body_identity)
                                    && normal.edge
                                        == UnitOwnerLoopEdgeV1::IfFallthrough(
                                            expected.body,
                                            control,
                                        )
                            }
                            (
                                UnitOwnerMixedArmBoundaryV1::Jump(actual),
                                LoopBranchExit::Continue(jump),
                            ) => std::sync::Arc::ptr_eq(
                                &actual.identity,
                                &self.closed.body_identity,
                            ) && actual.jump == jump
                                && actual.after_jump == self.entry
                                && actual.edge
                                    == UnitOwnerLoopEdgeV1::ContinueBackedge(
                                        crate::ownership_checking::UnitDropPoint::ControlTransfer(
                                            jump,
                                        ),
                                        self.condition(),
                                    ),
                            (
                                UnitOwnerMixedArmBoundaryV1::Jump(actual),
                                LoopBranchExit::Break(jump),
                            ) => std::sync::Arc::ptr_eq(
                                &actual.identity,
                                &self.closed.body_identity,
                            ) && actual.jump == jump
                                && actual.after_jump == self.entry
                                && actual.edge
                                    == UnitOwnerLoopEdgeV1::BreakExit(
                                        crate::ownership_checking::UnitDropPoint::ControlTransfer(
                                            jump,
                                        ),
                                        crate::ownership_checking::UnitDropPoint::LoopExit(
                                            self.statement(),
                                        ),
                                    ),
                            _ => false,
                        }
                })
    }
}
impl UnitOwnerObservation {
    /// Exactly one normal survivor and one actual jump; two-normal or two-terminal arms stay excluded.
    pub fn invariant_mixed_join_observation_v1(
        &self,
    ) -> Option<&UnitOwnerInvariantMixedJoinObservationV1> {
        let super::InvariantLoopValueV1::MixedJoin(value) = self.invariant_loop_value.as_deref()?
        else {
            return None;
        };
        (self.multi_root_boundary.is_none() && self.closed_loop_batch.is_none() && self.open_loop_value.is_none()
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
