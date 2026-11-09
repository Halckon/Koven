//! Nonempty boundary evidence, deliberately not a closed outer transport or a phi.
use super::super::{UnitOwnerId, UnitOwnerProducerCatalogV1, UnitOwnerValueObservationV1};
use super::{UnitOwnerLoopEdgeV1, UnitOwnerLoopObservationV1, UnitOwnerObservation};
use crate::name_resolution::UnitSymbolId;
use crate::type_checking::{UnitExpressionId, UnitStatementId};

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

/// An unchanged outer String owner at three independently observed While boundaries.
/// The body transport owns only its fresh local producers. This does not supply outer
/// suffix events, final consumption, a phi, dynamic instances, or a termination proof.
#[derive(Clone, Debug)]
pub struct UnitOwnerInvariantLoopObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) closed: UnitOwnerLoopObservationV1,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) backedge: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) exit: UnitOwnerLoopBindingV1,
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
    /// State read after actual body completion and scope cleanup.
    pub fn backedge(&self) -> &UnitOwnerLoopBindingV1 {
        &self.backedge
    }
    /// State read after actual LoopExit cleanup on the original condition state.
    pub fn exit(&self) -> &UnitOwnerLoopBindingV1 {
        &self.exit
    }
    /// Closed transport of body-local producers, with no access to the input owner.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        self.closed.body_transport()
    }
    /// Entry, normal backedge and condition-false exit; not a finite runtime trace.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        self.closed.edges()
    }
    /// Catalog and boundary identity consistency, not a full owner conservation proof.
    pub fn is_consistent(&self) -> bool {
        self.closed.is_consistent()
            && self.closed.completion == super::LoopBodyCompletion::Fallthrough
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
        None
    }
}
