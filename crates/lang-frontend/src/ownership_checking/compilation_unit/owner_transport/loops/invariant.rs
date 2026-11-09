//! Nonempty boundary evidence, deliberately not a closed outer transport or a phi.
mod boundaries;
mod breaks;
use super::super::{UnitOwnerId, UnitOwnerProducerCatalogV1, UnitOwnerValueObservationV1};
use super::{UnitOwnerLoopEdgeV1, UnitOwnerLoopObservationV1, UnitOwnerObservation};
use crate::name_resolution::UnitSymbolId;
use crate::type_checking::{UnitExpressionId, UnitStatementId};
pub use breaks::UnitOwnerInvariantBreakObservationV1;

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
/// The body transport owns only its fresh local producers. This does not supply outer
/// suffix events, final consumption, a phi, dynamic instances, or a termination proof.
#[derive(Clone, Debug)]
pub struct UnitOwnerInvariantLoopObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) closed: UnitOwnerLoopObservationV1,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) backedge: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) exit: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) continued:
        Option<(std::sync::Arc<()>, UnitExpressionId, UnitOwnerLoopBindingV1)>,
}
impl UnitOwnerInvariantLoopObservationV1 {
    /// Actual While statement.
    pub fn statement(&self) -> UnitStatementId {
        self.closed.statement()
    }
    /// Actual Boolean parameter condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.closed.condition()
    }
    /// Actual straight-line body.
    pub fn body(&self) -> UnitStatementId {
        self.closed.body()
    }
    /// Catalog of actual input producers, not proof of their final consumption.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.inputs
    }
    /// State read after actual condition evaluation, before body activation.
    pub fn entry(&self) -> &UnitOwnerLoopBindingV1 {
        &self.entry
    }
    /// State reread after body completion: normal scope cleanup, or the witnessed
    /// Continue cleanup followed by the body's actual early return.
    pub fn backedge(&self) -> &UnitOwnerLoopBindingV1 {
        &self.backedge
    }
    /// Actual Continue expression and independently read post-jump-cleanup binding.
    /// Normal body fallthrough has no Continue boundary.
    pub fn continue_boundary(&self) -> Option<(UnitExpressionId, &UnitOwnerLoopBindingV1)> {
        self.continued
            .as_ref()
            .map(|(_, expression, binding)| (*expression, binding))
    }
    /// State read after actual LoopExit cleanup on the original condition state.
    pub fn exit(&self) -> &UnitOwnerLoopBindingV1 {
        &self.exit
    }
    /// Closed transport of body-local producers, with no access to the input owner.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        self.closed.body_transport()
    }
    /// Entry, normal/Continue backedge and condition-false exit, not a runtime trace.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        self.closed.edges()
    }
    /// Catalog and boundary identity consistency, not a full owner conservation proof.
    pub fn is_consistent(&self) -> bool {
        let completion = match (self.closed.completion, &self.continued) {
            (super::LoopBodyCompletion::Fallthrough, None) => true,
            (super::LoopBodyCompletion::Continue(expected), Some((identity, actual, binding))) => {
                expected == *actual
                    && actual.source_unit() == self.statement().source_unit()
                    && *actual != self.condition()
                    && std::sync::Arc::ptr_eq(identity, &self.closed.body_identity)
                    && binding == &self.entry
            }
            _ => false,
        };
        self.closed.is_consistent()
            && completion
            && self.inputs.is_consistent()
            && self.inputs.definitions().len() == 1
            && self.inputs.callable() == self.closed.callable
            && std::sync::Arc::ptr_eq(&self.inputs.identity, &self.closed.identity)
            && self.entry == self.backedge
            && self.entry == self.exit
            && self.entry.symbol.source_unit() == self.statement().source_unit()
            && self.inputs.definition(self.entry.owner()).is_some()
    }
}
impl UnitOwnerObservation {
    /// Separate nonempty boundary capability. Legacy and empty-input loop APIs stay unchanged.
    pub fn invariant_loop_observation_v1(&self) -> Option<&UnitOwnerInvariantLoopObservationV1> {
        self.invariant_loop_value.as_deref().filter(|value| {
            self.loop_value.is_none()
                && matches!(self.status, super::super::UnitOwnerObservationStatus::Unsupported {
                    point: crate::ownership_checking::UnitDropPoint::AfterStatement(statement),
                    reason: super::super::UnitOwnerUnsupportedReason::Syntax
                } if statement == value.statement())
                && value.closed.callable == self.callable
                && std::sync::Arc::ptr_eq(&value.closed.identity, &self.identity)
                && value.is_consistent()
        })
    }
}
