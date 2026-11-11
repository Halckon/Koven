//! Nonempty boundary evidence, deliberately not a closed outer transport or a phi.
mod boundaries;
mod breaks;
mod mixed_join;
mod normal_join;
mod split_continue;
mod split_exits;
mod suffix;
use super::super::{UnitOwnerId, UnitOwnerProducerCatalogV1, UnitOwnerValueObservationV1};
use super::{UnitOwnerLoopEdgeV1, UnitOwnerLoopObservationV1, UnitOwnerObservation};
use crate::name_resolution::UnitSymbolId;
use crate::type_checking::{UnitExpressionId, UnitStatementId};
pub(in crate::ownership_checking::compilation_unit) use boundaries::InvariantLoopBoundaries;
pub use breaks::UnitOwnerInvariantBreakObservationV1;
pub use mixed_join::{UnitOwnerInvariantMixedJoinObservationV1, UnitOwnerMixedArmBoundaryV1};
pub use normal_join::{UnitOwnerInvariantNormalJoinObservationV1, UnitOwnerNormalArmBoundaryV1};
pub use split_continue::{
    UnitOwnerContinueArmBoundaryV1, UnitOwnerInvariantSplitContinueObservationV1,
};
pub use split_exits::{UnitOwnerInvariantSplitExitObservationV1, UnitOwnerJumpArmBoundaryV1};

/// One binding read from the actual planner state at a loop boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerLoopBindingV1 {
    pub(in crate::ownership_checking::compilation_unit) symbol: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) owner: UnitOwnerId,
}
impl UnitOwnerLoopBindingV1 {
    /// Actual named binding, source qualified.
    pub fn symbol(&self) -> UnitSymbolId {
        self.symbol
    }
    /// Real producer token; resolves only in this observation's input catalog.
    pub fn owner(&self) -> UnitOwnerId {
        self.owner.clone()
    }
}

/// An unchanged outer String owner at independently observed While boundaries.
/// The body transport owns only its fresh local producers. Basic boundaries alone do not
/// prove final consumption; the optional post-loop transport seals it separately. Neither
/// capability supplies a phi, dynamic instances, or a termination proof.
#[derive(Clone, Debug)]
pub struct UnitOwnerInvariantLoopObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) evidence: InvariantLoopBoundaries,
    pub(in crate::ownership_checking::compilation_unit) post_loop:
        Option<Box<suffix::PostLoopTransport>>,
}

#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) enum InvariantLoopValueV1 {
    Backedge(UnitOwnerInvariantLoopObservationV1),
    Break(UnitOwnerInvariantBreakObservationV1),
    SplitContinue(Box<UnitOwnerInvariantSplitContinueObservationV1>),
    SplitExit(Box<UnitOwnerInvariantSplitExitObservationV1>),
    NormalJoin(Box<UnitOwnerInvariantNormalJoinObservationV1>),
    MixedJoin(Box<UnitOwnerInvariantMixedJoinObservationV1>),
}
impl UnitOwnerInvariantLoopObservationV1 {
    /// Actual While statement.
    pub fn statement(&self) -> UnitStatementId {
        self.evidence.closed.statement()
    }
    /// Actual Boolean parameter condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.evidence.closed.condition()
    }
    /// Actual straight-line body.
    pub fn body(&self) -> UnitStatementId {
        self.evidence.closed.body()
    }
    /// Catalog of actual input producers, not proof of their final consumption.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.evidence.inputs
    }
    /// State read after actual condition evaluation, before body activation.
    pub fn entry(&self) -> &UnitOwnerLoopBindingV1 {
        &self.evidence.entry
    }
    /// State reread after body completion: normal scope cleanup, or the witnessed
    /// Continue cleanup followed by the body's actual early return.
    pub fn backedge(&self) -> &UnitOwnerLoopBindingV1 {
        &self.evidence.body_completion
    }
    /// Actual Continue expression and independently read post-jump-cleanup binding.
    /// Normal body fallthrough has no Continue boundary.
    pub fn continue_boundary(&self) -> Option<(UnitExpressionId, &UnitOwnerLoopBindingV1)> {
        self.evidence.transfer_boundary()
    }

    /// State read after actual LoopExit cleanup on the original condition state.
    pub fn exit(&self) -> &UnitOwnerLoopBindingV1 {
        &self.evidence.exit
    }
    /// Closed transport of body-local producers, with no access to the input owner.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        self.evidence.closed.body_transport()
    }
    /// Entry, normal/Continue backedge and condition-false exit, not a runtime trace.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        self.evidence.closed.edges()
    }
    /// Catalog and boundary identity consistency, not a full owner conservation proof.
    pub fn is_consistent(&self) -> bool {
        let completion = match (self.evidence.closed.completion, &self.evidence.transfer) {
            (super::LoopBodyCompletion::Fallthrough, None) => true,
            (super::LoopBodyCompletion::Continue(jump), Some(_)) => {
                self.evidence.transfer_matches(jump)
            }
            _ => false,
        };
        completion && self.evidence.is_consistent()
    }
}
impl UnitOwnerObservation {
    /// Separate nonempty boundary capability. Legacy and empty-input loop APIs stay unchanged.
    pub fn invariant_loop_observation_v1(&self) -> Option<&UnitOwnerInvariantLoopObservationV1> {
        let InvariantLoopValueV1::Backedge(value) = self.invariant_loop_value.as_deref()? else {
            return None;
        };
        (self.replace_break_value.is_none()
            && self.loop_value.is_none()
            && matches!(self.status, super::super::UnitOwnerObservationStatus::Unsupported {
                point: crate::ownership_checking::UnitDropPoint::AfterStatement(statement),
                reason: super::super::UnitOwnerUnsupportedReason::Syntax
            } if statement == value.statement())
            && value.evidence.closed.callable == self.callable
            && std::sync::Arc::ptr_eq(&value.evidence.closed.identity, &self.identity)
            && value.is_consistent())
        .then_some(value)
    }
}
