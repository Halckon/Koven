//! A Break exit is not a backedge and never exposes the backedge capability.
use super::boundaries::InvariantLoopBoundaries;
use super::{
    UnitExpressionId, UnitOwnerLoopBindingV1, UnitOwnerLoopEdgeV1, UnitOwnerObservation,
    UnitOwnerProducerCatalogV1, UnitOwnerValueObservationV1, UnitStatementId,
};

/// An invariant outer owner witnessed through a real terminal Break and loop exit.
/// The body completion state is independently checked before the original condition
/// state is used at LoopExit. This is not a phi, runtime trace or final consumption.
#[derive(Clone, Debug)]
pub struct UnitOwnerInvariantBreakObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) evidence: InvariantLoopBoundaries,
}
impl UnitOwnerInvariantBreakObservationV1 {
    /// Actual While statement targeted by the terminal Break.
    pub fn statement(&self) -> UnitStatementId {
        self.evidence.closed.statement()
    }
    /// Actual Boolean parameter condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.evidence.closed.condition()
    }
    /// Actual body that returned false after Break cleanup, not normal scope completion.
    pub fn body(&self) -> UnitStatementId {
        self.evidence.closed.body()
    }
    /// Real input producer catalog, not an outer transport conservation proof.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.evidence.inputs
    }
    /// Binding read after actual condition evaluation.
    pub fn entry(&self) -> &UnitOwnerLoopBindingV1 {
        &self.evidence.entry
    }
    /// Actual Break expression and independently read post-jump-cleanup binding.
    pub fn break_boundary(&self) -> Option<(UnitExpressionId, &UnitOwnerLoopBindingV1)> {
        self.evidence.transfer_boundary()
    }
    /// Binding reread after body Ok(false), before discarding its checked state.
    pub fn body_completion(&self) -> &UnitOwnerLoopBindingV1 {
        &self.evidence.body_completion
    }
    /// Binding read after LoopExit cleanup on the original condition state.
    pub fn exit(&self) -> &UnitOwnerLoopBindingV1 {
        &self.evidence.exit
    }
    /// Closed body-local transport, independent of the input owner catalog.
    pub fn body_transport(&self) -> &UnitOwnerValueObservationV1 {
        self.evidence.closed.body_transport()
    }
    /// EnterBody, actual BreakExit and ConditionFalseExit; no backedge.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        self.evidence.closed.edges()
    }
    /// Private identity/control seal, not general loop admission or final consumption.
    pub fn is_consistent(&self) -> bool {
        matches!(self.evidence.closed.completion, super::super::LoopBodyCompletion::Break(jump)
            if self.evidence.transfer_matches(jump))
            && self.evidence.is_consistent()
    }
}
impl UnitOwnerObservation {
    /// Separate invariant Break exit capability; the old loop backedge contract stays strict.
    pub fn invariant_break_observation_v1(&self) -> Option<&UnitOwnerInvariantBreakObservationV1> {
        let super::InvariantLoopValueV1::Break(value) = self.invariant_loop_value.as_deref()?
        else {
            return None;
        };
        (self.loop_value.is_none()
            && matches!(self.status, crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
                point: crate::ownership_checking::UnitDropPoint::AfterStatement(statement),
                reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax
            } if statement == value.statement())
            && value.evidence.closed.callable == self.callable
            && std::sync::Arc::ptr_eq(&value.evidence.closed.identity, &self.identity)
            && value.is_consistent()).then_some(value)
    }
}
