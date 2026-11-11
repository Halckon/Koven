//! Independently observed normal arms, real If join and normal body backedge.
use super::super::{
    LoopBodyCompletion, LoopBranchExit, UnitOwnerLoopEdgeV1, UnitOwnerLoopObservationV1,
};
use super::{
    UnitExpressionId, UnitOwnerLoopBindingV1, UnitOwnerObservation, UnitOwnerProducerCatalogV1,
    UnitOwnerValueObservationV1, UnitStatementId,
};

/// Independently read entry and post-cleanup completion of one normal empty arm.
#[derive(Clone, Debug)]
pub struct UnitOwnerNormalArmBoundaryV1 {
    pub(in crate::ownership_checking::compilation_unit) identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) control: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) branch: usize,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) edge: UnitOwnerLoopEdgeV1,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) completion: UnitOwnerLoopBindingV1,
}
impl UnitOwnerNormalArmBoundaryV1 {
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
    /// Normal completion returns to this arm's actual If, not directly to the loop.
    pub fn edge(&self) -> UnitOwnerLoopEdgeV1 {
        self.edge
    }
    /// Read after actual branch-entry cleanup.
    pub fn entry(&self) -> &UnitOwnerLoopBindingV1 {
        &self.entry
    }
    /// Read after the arm returned true and its actual branch-exit cleanup.
    pub fn completion(&self) -> &UnitOwnerLoopBindingV1 {
        &self.completion
    }
}

/// Unchanged input on two normal empty arms, their real merge, and the body backedge.
/// This proves only identical singleton states, not a general phi or changing owner join.
#[derive(Clone, Debug)]
pub struct UnitOwnerInvariantNormalJoinObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) closed: UnitOwnerLoopObservationV1,
    pub(in crate::ownership_checking::compilation_unit) control: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) branch_condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) split_identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) split: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) arms: [UnitOwnerNormalArmBoundaryV1; 2],
    pub(in crate::ownership_checking::compilation_unit) join_identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) join: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) backedge: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) exit: UnitOwnerLoopBindingV1,
}
impl UnitOwnerInvariantNormalJoinObservationV1 {
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
    pub fn arms(&self) -> &[UnitOwnerNormalArmBoundaryV1; 2] {
        &self.arms
    }
    /// Independently read after the actual two-state If merge.
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
    /// Entry, two normal If arms, one body backedge, and condition-false exit.
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
            .all(|branch| branch.exit == LoopBranchExit::Fallthrough)
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
            && self.inputs.definition(self.entry.owner()).is_some()
            && self
                .arms
                .iter()
                .zip(branches)
                .enumerate()
                .all(|(index, (arm, expected))| {
                    arm.edge == UnitOwnerLoopEdgeV1::IfFallthrough(expected.body, control)
                        && arm.control == control
                        && arm.branch == index
                        && arm.body == expected.body
                        && std::sync::Arc::ptr_eq(&arm.identity, &self.closed.body_identity)
                        && arm.entry == self.entry
                        && arm.completion == self.entry
                })
    }
}
impl UnitOwnerObservation {
    /// Separate two-normal-arm singleton join capability; mixed or terminal arms stay excluded.
    pub fn invariant_normal_join_observation_v1(
        &self,
    ) -> Option<&UnitOwnerInvariantNormalJoinObservationV1> {
        let super::InvariantLoopValueV1::NormalJoin(value) =
            self.invariant_loop_value.as_deref()?
        else {
            return None;
        };
        (self.replace_break_value.is_none()
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
