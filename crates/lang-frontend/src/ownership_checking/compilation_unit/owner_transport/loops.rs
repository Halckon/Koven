//! A closed body is a repeatable template, never a carried owner or a termination proof.
mod multi_root_header;
pub use multi_root_header::UnitOwnerRootExitPartitionV1;
mod multi_root_normal;
pub use multi_root_normal::{UnitOwnerMultiRootNormalTransportV1, UnitOwnerRootReplaceStepV1};
mod multi_root_boundary;
pub use multi_root_boundary::{UnitOwnerMultiRootBoundaryV1, UnitOwnerRootBoundarySlotV1};
mod closed_batch;
pub use closed_batch::UnitOwnerClosedLoopBatchV1;
mod invariant;
mod normal_backedge;
mod qualified_edge;
pub use qualified_edge::UnitOwnerRootEdgeV1;
mod open;
pub(in crate::ownership_checking::compilation_unit) use normal_backedge::{
    HeaderCompletion, HeaderCore, HeaderEquation, MixedParameterThreshold,
};
pub use normal_backedge::{
    UnitOwnerConditionalBreakBackedgeV1, UnitOwnerConditionalReturnBackedgeV1,
    UnitOwnerContinueBackedgeV1, UnitOwnerHeaderSourceV1, UnitOwnerMixedBackedgeV1,
    UnitOwnerNormalBackedgeV1, UnitOwnerNormalBindDropV1,
};
pub use open::{
    UnitOwnerOpenLoopObservationV1, UnitOwnerOpenLoopPrefixV1, UnitOwnerOpenLoopReturnV1,
    UnitOwnerReturnActionV1, UnitOwnerReturnEndpointV1,
};
mod replace_break;
use super::{UnitOwnerCallable, UnitOwnerObservation, UnitOwnerValueObservationV1};
use crate::ownership_checking::UnitDropPoint;
use crate::type_checking::{UnitExpressionId, UnitStatementId};
pub(in crate::ownership_checking::compilation_unit) use invariant::{
    InvariantLoopBoundaries, InvariantLoopValueV1,
};
pub use invariant::{
    UnitOwnerContinueArmBoundaryV1, UnitOwnerInvariantBreakObservationV1,
    UnitOwnerInvariantLoopObservationV1, UnitOwnerInvariantMixedJoinObservationV1,
    UnitOwnerInvariantNormalJoinObservationV1, UnitOwnerInvariantSplitContinueObservationV1,
    UnitOwnerInvariantSplitExitObservationV1, UnitOwnerJumpArmBoundaryV1, UnitOwnerLoopBindingV1,
    UnitOwnerMixedArmBoundaryV1, UnitOwnerNormalArmBoundaryV1,
};
pub(in crate::ownership_checking::compilation_unit) use replace_break::{
    CallConsumerTarget, ReplaceBreakSuffix, ReturnConsumerTarget,
};
pub use replace_break::{
    UnitOwnerReplaceBreakCallActionV1, UnitOwnerReplaceBreakCallConsumerV1,
    UnitOwnerReplaceBreakConsumerActionV1, UnitOwnerReplaceBreakConsumerV1,
    UnitOwnerReplaceBreakObservationV1, UnitOwnerReplaceBreakReturnActionV1,
    UnitOwnerReplaceBreakReturnConsumerV1, UnitOwnerReplaceBreakStateV1,
};

/// Structural control edges of a sealed While body, not an executed finite trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerLoopEdgeV1 {
    /// The real condition's true edge enters the actual body.
    EnterBody(UnitExpressionId, UnitStatementId),
    /// The false edge exits without activating any body definitions.
    ConditionFalseExit(UnitExpressionId, UnitDropPoint),
    /// Actual body fallthrough after scope cleanup returns to this condition.
    Backedge(UnitDropPoint, UnitExpressionId),
    /// A real terminal Continue reaches this condition after jump cleanup.
    ContinueBackedge(UnitDropPoint, UnitExpressionId),
    /// A real terminal Break reaches this loop exit after jump cleanup.
    BreakExit(UnitDropPoint, UnitDropPoint),
    /// A real empty arm returns normally to its containing If, not a Drop point.
    IfFallthrough(UnitStatementId, UnitExpressionId),
    /// One mutually exclusive arm selected by the actual terminal If condition.
    IfBranch {
        /// Actual If expression containing this branch.
        control: UnitExpressionId,
        /// Actual independent Boolean condition.
        condition: UnitExpressionId,
        /// Ordered arm, zero for then and one for else.
        branch: usize,
        /// Actual arm statement whose terminal jump was observed.
        body: UnitStatementId,
    },
}

// Kept separately from public edges: only actual completion supplies the sealed endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) enum LoopBranchExit {
    Fallthrough,
    Continue(UnitExpressionId),
    Break(UnitExpressionId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) struct LoopBranchCompletion {
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) exit: LoopBranchExit,
}
impl LoopBranchCompletion {
    pub(in crate::ownership_checking::compilation_unit) fn continues(self) -> bool {
        self.exit == LoopBranchExit::Fallthrough
    }
    pub(in crate::ownership_checking::compilation_unit) fn jump(self) -> Option<UnitExpressionId> {
        match self.exit {
            LoopBranchExit::Fallthrough => None,
            LoopBranchExit::Continue(jump) | LoopBranchExit::Break(jump) => Some(jump),
        }
    }
    pub(in crate::ownership_checking::compilation_unit) fn completion(
        self,
    ) -> Option<LoopBodyCompletion> {
        match self.exit {
            LoopBranchExit::Fallthrough => None,
            LoopBranchExit::Continue(jump) => Some(LoopBodyCompletion::Continue(jump)),
            LoopBranchExit::Break(jump) => Some(LoopBodyCompletion::Break(jump)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) enum LoopBodyCompletion {
    Fallthrough,
    Continue(UnitExpressionId),
    Break(UnitExpressionId),
    ConditionalBreak {
        control: UnitExpressionId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        jump: UnitExpressionId,
    },
    ConditionalReturn {
        control: UnitExpressionId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        expression: UnitExpressionId,
    },
    Conditional {
        control: UnitExpressionId,
        condition: UnitExpressionId,
        branches: [LoopBranchCompletion; 2],
    },
}
impl LoopBodyCompletion {
    pub(in crate::ownership_checking::compilation_unit) fn continues(self) -> bool {
        match self {
            Self::Fallthrough | Self::ConditionalBreak { .. } | Self::ConditionalReturn { .. } => {
                true
            }
            Self::Continue(_) | Self::Break(_) => false,
            Self::Conditional { branches, .. } => branches.iter().any(|branch| branch.continues()),
        }
    }
    pub(in crate::ownership_checking::compilation_unit) fn edges(
        self,
        statement: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
    ) -> Vec<UnitOwnerLoopEdgeV1> {
        let mut edges = vec![UnitOwnerLoopEdgeV1::EnterBody(condition, body)];
        match self {
            Self::Fallthrough => edges.push(UnitOwnerLoopEdgeV1::Backedge(
                UnitDropPoint::AfterStatement(body),
                condition,
            )),
            Self::Continue(jump) => edges.push(UnitOwnerLoopEdgeV1::ContinueBackedge(
                UnitDropPoint::ControlTransfer(jump),
                condition,
            )),
            Self::Break(jump) => edges.push(UnitOwnerLoopEdgeV1::BreakExit(
                UnitDropPoint::ControlTransfer(jump),
                UnitDropPoint::LoopExit(statement),
            )),
            Self::ConditionalBreak {
                control,
                condition: branch_condition,
                body: arm,
                jump,
            } => {
                edges.push(UnitOwnerLoopEdgeV1::IfBranch {
                    control,
                    condition: branch_condition,
                    branch: 0,
                    body: arm,
                });
                edges.push(UnitOwnerLoopEdgeV1::BreakExit(
                    UnitDropPoint::ControlTransfer(jump),
                    UnitDropPoint::LoopExit(statement),
                ));
                edges.push(UnitOwnerLoopEdgeV1::Backedge(
                    UnitDropPoint::AfterStatement(body),
                    condition,
                ));
            }
            // This variant never projects into the legacy closed-loop edge model.
            Self::ConditionalReturn { .. } => {
                return Vec::new();
            }
            Self::Conditional {
                control,
                condition: branch_condition,
                branches,
            } => {
                for (branch, completed) in branches.into_iter().enumerate() {
                    edges.push(UnitOwnerLoopEdgeV1::IfBranch {
                        control,
                        condition: branch_condition,
                        branch,
                        body: completed.body,
                    });
                    edges.push(match completed.exit {
                        LoopBranchExit::Fallthrough => {
                            UnitOwnerLoopEdgeV1::IfFallthrough(completed.body, control)
                        }
                        LoopBranchExit::Continue(jump) => UnitOwnerLoopEdgeV1::ContinueBackedge(
                            UnitDropPoint::ControlTransfer(jump),
                            condition,
                        ),
                        LoopBranchExit::Break(jump) => UnitOwnerLoopEdgeV1::BreakExit(
                            UnitDropPoint::ControlTransfer(jump),
                            UnitDropPoint::LoopExit(statement),
                        ),
                    });
                }
                if self.continues() {
                    edges.push(UnitOwnerLoopEdgeV1::Backedge(
                        UnitDropPoint::AfterStatement(body),
                        condition,
                    ));
                }
            }
        }
        edges.push(UnitOwnerLoopEdgeV1::ConditionFalseExit(
            condition,
            UnitDropPoint::LoopExit(statement),
        ));
        edges
    }
}

/// A real closed While body whose definitions are fresh on every activation.
/// Definition tokens identify static producers in this template, not runtime instances.
/// The initial pure Boolean parameter is unchanged: false means zero activations;
/// true may repeat forever for fallthrough/Continue, while a terminal Break exits after
/// one activation. A terminal If independently selects a jump or an empty normal arm.
/// No runtime iteration trace or carried owner is represented.
#[derive(Clone, Debug)]
pub struct UnitOwnerLoopObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) body_identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) completion: LoopBodyCompletion,
    pub(in crate::ownership_checking::compilation_unit) transport: UnitOwnerValueObservationV1,
    pub(in crate::ownership_checking::compilation_unit) edges: Vec<UnitOwnerLoopEdgeV1>,
}
impl UnitOwnerLoopObservationV1 {
    /// Source-qualified loop statement, never a producer identity.
    pub fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// Actual pure Boolean condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual body statement whose cleanup precedes its fallthrough or jump edge.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Closed straight-line owner transport for one fresh activation, with no incoming owners.
    /// A terminal If has no owner events; its exclusive control edges are stored separately.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        &self.transport
    }
    /// Structural edges established by real traversal and cleanup.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        &self.edges
    }
    /// Internal identity, edge and conservation consistency, not general loop admission.
    pub fn is_consistent(&self) -> bool {
        let UnitOwnerCallable::Function(function) = self.callable else {
            return false;
        };
        let source = function.source_unit();
        let completion_valid = match self.completion {
            LoopBodyCompletion::ConditionalBreak { .. }
            | LoopBodyCompletion::ConditionalReturn { .. } => false,
            LoopBodyCompletion::Fallthrough => true,
            LoopBodyCompletion::Continue(jump) | LoopBodyCompletion::Break(jump) => {
                jump.source_unit() == source
            }
            LoopBodyCompletion::Conditional {
                control,
                condition,
                branches,
            } => {
                control.source_unit() == source
                    && condition.source_unit() == source
                    && control != condition
                    && condition != self.condition
                    && branches[0].body != branches[1].body
                    && (branches[0].jump().is_none() || branches[0].jump() != branches[1].jump())
                    && branches.iter().all(|branch| {
                        branch.body.source_unit() == source
                            && branch.body != self.body
                            && branch.body != self.statement
                            && branch.jump().is_none_or(|jump| {
                                jump.source_unit() == source
                                    && jump != control
                                    && jump != condition
                                    && jump != self.condition
                            })
                    })
            }
        };
        completion_valid
            && self.statement.source_unit() == source
            && self.condition.source_unit() == source
            && self.body.source_unit() == source
            && self.statement != self.body
            && std::sync::Arc::ptr_eq(&self.body_identity, &self.transport.catalog.identity)
            && !std::sync::Arc::ptr_eq(&self.body_identity, &self.identity)
            && self.transport.producers().callable() == self.callable
            && !self.transport.producers().definitions().is_empty()
            && self.transport.choice().is_none()
            && self.transport.suffix().is_empty()
            && self.transport.is_consistent()
            && self.edges
                == self
                    .completion
                    .edges(self.statement, self.condition, self.body)
    }
}
impl UnitOwnerObservation {
    /// Separate sealed loop capability; legacy status/events and Choice remain unchanged.
    pub fn loop_observation_v1(&self) -> Option<&UnitOwnerLoopObservationV1> {
        self.loop_value.as_deref().filter(|value| {
            self.multi_root_boundary.is_none() && self.closed_loop_batch.is_none() && self.open_loop_value.is_none() && self.invariant_loop_value.is_none() && matches!(self.status, super::UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterStatement(statement), reason: super::UnitOwnerUnsupportedReason::Syntax
            } if statement == value.statement)
                && value.callable == self.callable
                && std::sync::Arc::ptr_eq(&value.identity, &self.identity)
                && value.is_consistent()
        })
    }
}

// Keep the legacy publication policy, but no sealed loop survives a deferred product.
pub(in crate::ownership_checking::compilation_unit) fn publish_observations(
    mut observations: Vec<UnitOwnerObservation>,
    successful: bool,
    complete: bool,
) -> Vec<UnitOwnerObservation> {
    if !successful {
        return Vec::new();
    }
    if !complete {
        for observation in &mut observations {
            observation.closed_loop_batch = None;
            observation.multi_root_boundary = None;
            observation.open_loop_value = None;
            observation.loop_value = None;
            observation.invariant_loop_value = None;
        }
    }
    observations
}
