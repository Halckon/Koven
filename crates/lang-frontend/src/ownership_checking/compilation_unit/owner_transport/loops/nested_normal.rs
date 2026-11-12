//! Normal-body permission only, anchored to an actual parent/child traversal.
use super::super::{UnitOwnerObservationStatus, UnitOwnerUnsupportedReason};
use super::*;
/// A completed inner normal-body rule, without entry, exit, or outer-loop authority.
#[derive(Clone, Debug)]
pub struct UnitOwnerNestedNormalBodyV1 {
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    sites: (
        UnitStatementId,
        UnitStatementId,
        UnitExpressionId,
        UnitStatementId,
    ),
    normal: UnitOwnerMultiRootNormalTransportV1,
}
impl UnitOwnerNestedNormalBodyV1 {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        identity: std::sync::Arc<()>,
        callable: UnitOwnerCallable,
        sites: (
            UnitStatementId,
            UnitStatementId,
            UnitExpressionId,
            UnitStatementId,
        ),
        normal: UnitOwnerMultiRootNormalTransportV1,
    ) -> Option<Self> {
        let value = Self {
            identity,
            callable,
            sites,
            normal,
        };
        value.is_consistent().then_some(value)
    }
    /// Actual enclosing While recorded by the traversal control stack.
    pub fn parent(&self) -> UnitStatementId {
        self.sites.0
    }
    /// Actual immediately nested While whose normal body was completed.
    pub fn control(&self) -> UnitStatementId {
        self.sites.1
    }
    /// Actual child condition completed before its body-entry event.
    pub fn condition(&self) -> UnitExpressionId {
        self.sites.2
    }
    /// Actual child body entered and completed through the planner.
    pub fn body(&self) -> UnitStatementId {
        self.sites.3
    }
    /// Formal Header rule populated by actual producer, Replace, binding and Drop events.
    pub fn normal(&self) -> &UnitOwnerMultiRootNormalTransportV1 {
        &self.normal
    }
    /// Check local identity and formal-rule consistency, not whole-loop induction.
    pub fn is_consistent(&self) -> bool {
        self.parent() != self.control()
            && self.control() != self.body()
            && self.parent().source_unit() == self.control().source_unit()
            && self.condition().source_unit() == self.control().source_unit()
            && self.body().source_unit() == self.control().source_unit()
            && self.normal.is_consistent()
            && self.normal.statement() == self.control()
            && self.normal.body_catalog().callable() == self.callable
    }
    pub(in crate::ownership_checking::compilation_unit) fn matches(
        &self,
        o: &UnitOwnerObservation,
    ) -> bool {
        self.is_consistent()
            && std::sync::Arc::ptr_eq(&self.identity, &o.identity)
            && self.callable == o.callable
            && o.definitions.is_empty()
            && o.events.is_empty()
            && o.private_control_chain.is_none()
            && o.control_bridge.is_none()
            && o.counter_batch.is_none()
            && o.counter_tail.is_none()
            && o.closed_loop_batch.is_none()
            && o.multi_root_boundary.is_none()
            && o.loop_value.is_none()
            && o.open_loop_value.is_none()
            && o.invariant_loop_value.is_none()
            && o.choice_value.is_none()
            && matches!(o.status,UnitOwnerObservationStatus::Unsupported {point:UnitDropPoint::AfterStatement(c)|UnitDropPoint::LoopExit(c),reason:UnitOwnerUnsupportedReason::Syntax} if c==self.parent())
    }
}
impl UnitOwnerObservation {
    /// Return the isolated nested body permission only for this matching observation.
    pub fn nested_normal_body_v1(&self) -> Option<&UnitOwnerNestedNormalBodyV1> {
        self.nested_normal.as_deref().filter(|n| n.matches(self))
    }
}
