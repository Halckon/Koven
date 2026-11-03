//! Narrow owner identity observations, independent of cleanup eligibility.
use super::UnitDropPoint;
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;

/// Callable whose straight-line evaluation owns this observation transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerCallable {
    /// Ordinary function body.
    Function(UnitItemId),
    /// Lambda body (not supported by this initial observation slice).
    Lambda(UnitExpressionId),
    /// Deinitializer body (not supported by this initial observation slice).
    Deinit(UnitItemId),
}

/// Opaque definition token; compare only within one ownership analysis product.
/// It is not a runtime allocation identity or a source-span identity.
#[derive(Clone)]
pub struct UnitOwnerId {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitOwnerCallable,
    pub(super) index: usize,
}

impl std::fmt::Debug for UnitOwnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnitOwnerId")
            .field("callable", &self.callable)
            .field("index", &self.index)
            .finish()
    }
}
impl PartialEq for UnitOwnerId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.callable == other.callable
            && self.index == other.index
    }
}
impl Eq for UnitOwnerId {}

/// Actual producer of a complete plain owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerOrigin {
    /// Value parameter admitted at function entry.
    Parameter(UnitSymbolId),
    /// Completed non-interpolated String construction.
    String(UnitExpressionId),
}

/// One owner definition, never synthesized by Name, binding or replace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerDefinition {
    pub(super) id: UnitOwnerId,
    pub(super) origin: UnitOwnerOrigin,
}
impl UnitOwnerDefinition {
    /// Token local to this analysis product.
    pub fn id(&self) -> UnitOwnerId {
        self.id.clone()
    }
    /// Source-qualified construction or entry parameter.
    pub fn origin(&self) -> UnitOwnerOrigin {
        self.origin
    }
}

/// Reason this observation cannot describe a whole callable; not a language error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerUnsupportedReason {
    /// Branch, loop, destructuring, capture, assignment or another unmodelled node.
    Syntax,
    /// A call without the supported checked root-replace contract.
    OpaqueCall,
    /// The required complete plain owner was not available at the actual transition.
    MissingOwner,
    /// Internal observation conservation check failed; legacy ownership remains independent.
    InconsistentTransport,
    /// The callable has an implicit receiver or unsupported parameter shape.
    Callable,
}

/// Observation capability is separate from ownership/cleanup validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerObservationStatus {
    /// Every visited transition in this callable belongs to the narrow slice.
    Complete,
    /// All partial definitions and events have been discarded.
    Unsupported {
        /// First unsupported source-qualified execution point.
        point: UnitDropPoint,
        /// Unsupported capability, not an ownership diagnostic.
        reason: UnitOwnerUnsupportedReason,
    },
}

/// Events emitted at real evaluation/registration/commit transitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitOwnerTransportEvent {
    /// A consuming Name removed this exact current binding value.
    Move {
        /// Source-qualified evaluated expression.
        expression: UnitExpressionId,
        /// Source-qualified named binding.
        symbol: UnitSymbolId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// A real named cleanup removed the current value (not a source of identity).
    Drop {
        /// Actual cleanup execution point.
        point: UnitDropPoint,
        /// Source-qualified named binding.
        symbol: UnitSymbolId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// An expression produced or forwarded an owner into its caller's local result.
    Result {
        /// Source-qualified evaluated expression.
        expression: UnitExpressionId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// A new binding received the initializer's existing owner.
    Binding {
        /// Source-qualified named binding.
        symbol: UnitSymbolId,
        /// Expression whose result was accepted by this binding.
        initializer: UnitExpressionId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// An evaluated Value operand was registered in this call's pending slot.
    Pending {
        /// Checked call identity, isolated from nested calls.
        call: UnitExpressionId,
        /// Exact evaluated Value operand identity.
        argument: UnitExpressionId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// Atomic checked replacement: root receives incoming; result receives previous.
    Replace {
        /// Checked call identity, isolated from nested calls.
        call: UnitExpressionId,
        /// Complete named root updated by the commit.
        root: UnitSymbolId,
        /// Exact evaluated Value operand identity.
        argument: UnitExpressionId,
        /// Root owner before commit; also the result owner.
        previous: UnitOwnerId,
        /// Pending RHS owner; also the root owner after commit.
        incoming: UnitOwnerId,
    },
}

/// Whole-callable atomic observation. Unsupported observations expose no prefix facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerObservation {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitOwnerCallable,
    pub(super) status: UnitOwnerObservationStatus,
    pub(super) definitions: Vec<UnitOwnerDefinition>,
    pub(super) events: Vec<UnitOwnerTransportEvent>,
}
impl UnitOwnerObservation {
    /// Source-qualified callable.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Whether the entire callable was observed.
    pub fn status(&self) -> UnitOwnerObservationStatus {
        self.status
    }
    /// Actual owner definitions; empty when unsupported.
    pub fn definitions(&self) -> &[UnitOwnerDefinition] {
        &self.definitions
    }
    /// Actual state-transition events; empty when unsupported.
    pub fn events(&self) -> &[UnitOwnerTransportEvent] {
        &self.events
    }
    /// Resolve a token only in its owning callable observation.
    pub fn definition(&self, id: UnitOwnerId) -> Option<&UnitOwnerDefinition> {
        self.definitions
            .get(id.index)
            .filter(|definition| definition.id == id)
    }
}
