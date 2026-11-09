//! A closed body is a repeatable template, never a carried owner or a termination proof.
use super::{UnitOwnerCallable, UnitOwnerObservation, UnitOwnerValueObservationV1};
use crate::ownership_checking::UnitDropPoint;
use crate::type_checking::{UnitExpressionId, UnitStatementId};

/// Structural control edges of a sealed While body, not an executed finite trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerLoopEdgeV1 {
    /// The real condition's true edge enters the actual body.
    EnterBody(UnitExpressionId, UnitStatementId),
    /// The false edge exits without activating any body definitions.
    ConditionFalseExit(UnitExpressionId, UnitDropPoint),
    /// Actual body fallthrough after scope cleanup returns to this condition.
    Backedge(UnitDropPoint, UnitExpressionId),
}

/// A real closed While body whose definitions are fresh on every activation.
/// Definition tokens identify static producers in this template, not runtime instances.
/// The initial pure Boolean parameter is unchanged: false means zero activations;
/// true may repeat forever. Finite prefixes do not establish a terminating execution.
#[derive(Clone, Debug)]
pub struct UnitOwnerLoopObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) body_identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) transport: UnitOwnerValueObservationV1,
    pub(in crate::ownership_checking::compilation_unit) edges: [UnitOwnerLoopEdgeV1; 3],
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
    /// Actual body statement whose cleanup precedes the backedge.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Closed straight-line transport for one fresh activation, with no incoming owners.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        &self.transport
    }
    /// Structural edges established by real traversal and cleanup.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1; 3] {
        &self.edges
    }
    /// Internal identity, edge and conservation consistency, not general loop admission.
    pub fn is_consistent(&self) -> bool {
        let UnitOwnerCallable::Function(function) = self.callable else {
            return false;
        };
        let source = function.source_unit();
        self.statement.source_unit() == source
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
                == [
                    UnitOwnerLoopEdgeV1::EnterBody(self.condition, self.body),
                    UnitOwnerLoopEdgeV1::Backedge(
                        UnitDropPoint::AfterStatement(self.body),
                        self.condition,
                    ),
                    UnitOwnerLoopEdgeV1::ConditionFalseExit(
                        self.condition,
                        UnitDropPoint::LoopExit(self.statement),
                    ),
                ]
    }
}
impl UnitOwnerObservation {
    /// Separate sealed loop capability; legacy status/events and Choice remain unchanged.
    pub fn loop_observation_v1(&self) -> Option<&UnitOwnerLoopObservationV1> {
        self.loop_value.as_deref().filter(|value| {
            matches!(self.status, super::UnitOwnerObservationStatus::Unsupported {
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
            observation.loop_value = None;
        }
    }
    observations
}
