//! Normal-body permission only, anchored to an actual parent/child traversal.
use super::super::{UnitOwnerObservationStatus, UnitOwnerUnsupportedReason};
use super::*;
use crate::ownership_checking::compilation_unit::UnitSymbolId;
use crate::ownership_checking::{UnitDropFact, UnitDropTarget};
type CounterTail = (UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4]);
type NestedCompletion = (
    Option<UnitOwnerNestedResumeWriteV1>,
    Option<UnitOwnerNestedParentBoundaryV1>,
    Option<CounterTail>,
);
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
    exit_partition: Option<(Vec<UnitSymbolId>, Vec<UnitDropFact>)>,
    resume_roots: Option<Vec<UnitSymbolId>>,
    resume_write: Option<UnitOwnerNestedResumeWriteV1>,
    parent_boundary: Option<UnitOwnerNestedParentBoundaryV1>,
    counter_tail: Option<CounterTail>,
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
        exit_partition: Option<(Vec<UnitSymbolId>, Vec<UnitDropFact>)>,
        resume_roots: Option<Vec<UnitSymbolId>>,
        parent_facts: NestedCompletion,
    ) -> Option<Self> {
        let (resume_write, parent_boundary, counter_tail) = parent_facts;
        let value = Self {
            identity,
            callable,
            sites,
            normal,
            exit_partition,
            resume_roots,
            resume_write,
            parent_boundary,
            counter_tail,
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
    /// Actual child condition-false cleanup partition: surviving roots and fresh
    /// LoopExit drops. Static candidate identities, not a selected runtime generation
    /// or permission to consume roots in the enclosing control.
    pub fn false_exit_partition(&self) -> Option<(&[UnitSymbolId], &[UnitDropFact])> {
        self.exit_partition
            .as_ref()
            .map(|(live, drops)| (live.as_slice(), drops.as_slice()))
    }
    /// Actual traversal return to the declared parent after child false cleanup.
    /// This boundary fact alone is not permission for any resumed consumer.
    pub fn parent_resume_roots(&self) -> Option<&[UnitSymbolId]> {
        self.resume_roots.as_deref()
    }
    /// Completed single-statement facts; no backend resume permission is implied.
    pub fn parent_resume_write(&self) -> Option<&UnitOwnerNestedResumeWriteV1> {
        self.resume_write.as_ref()
    }
    /// Actual parent boundary observations only, not a generational invariant or backend authority.
    pub fn parent_boundary_v1(&self) -> Option<&UnitOwnerNestedParentBoundaryV1> {
        self.parent_boundary.as_ref()
    }
    pub(super) fn generation_counter_tail(&self) -> Option<CounterTail> {
        self.counter_tail
    }
    /// Conditional source-level child recurrence; no new lowering authority is granted.
    pub fn child_generation_rule_v1(&self) -> Option<UnitOwnerChildGenerationRuleV1> {
        UnitOwnerChildGenerationRuleV1::from_nested(self)
    }
    fn resume_is_consistent(&self) -> bool {
        if self.resume_write.as_ref().is_some_and(|w| {
            self.resume_roots
                .as_ref()
                .is_none_or(|r| !w.is_consistent(self.parent(), r))
        }) {
            return false;
        }
        let Some(roots) = &self.resume_roots else {
            return true;
        };
        let Some((live, _)) = &self.exit_partition else {
            return false;
        };
        roots.len() == live.len()
            && roots
                .iter()
                .enumerate()
                .all(|(i, r)| !roots[..i].contains(r) && live.contains(r))
    }
    fn exit_is_consistent(&self) -> bool {
        let Some((live, drops)) = &self.exit_partition else {
            return true;
        };
        let roots = self.normal.header_roots();
        live.len() + drops.len() == roots.len()
            && live
                .iter()
                .enumerate()
                .all(|(i, r)| roots.contains(r) && !live[..i].contains(r))
            && roots.iter().all(|r| {
                usize::from(live.contains(r))
                    + drops
                        .iter()
                        .filter(|d| {
                            d.target() == UnitDropTarget::Named(*r)
                                && d.point() == UnitDropPoint::LoopExit(self.control())
                        })
                        .count()
                    == 1
            })
    }
    /// Check local identity and formal-rule consistency, not whole-loop induction.
    pub fn is_consistent(&self) -> bool {
        self.parent() != self.control()
            && self.control() != self.body()
            && self.parent().source_unit() == self.control().source_unit()
            && self.condition().source_unit() == self.control().source_unit()
            && self.body().source_unit() == self.control().source_unit()
            && self.parent_boundary.as_ref().is_none_or(|p| {
                p.control() == self.parent()
                    && p.is_consistent()
                    && p.roots() == self.normal.header_roots()
            })
            && self.resume_is_consistent()
            && self.exit_is_consistent()
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
