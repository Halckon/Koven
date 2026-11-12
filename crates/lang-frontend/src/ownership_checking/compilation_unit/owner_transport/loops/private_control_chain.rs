//! Atomic three-control evidence with a borrowed normal-body-only group projection.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::{UnitDropFact, UnitDropTarget};
use std::sync::Arc;

// BodyPlan completion order is guard, counter, assignment sites.
type Tail = (UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4]);
type Entry = (
    [UnitSymbolId; 2],
    [UnitExpressionId; 2],
    [UnitStatementId; 2],
);
#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct PrivateChainMember {
    control: UnitStatementId,
    condition: UnitExpressionId,
    body: UnitStatementId,
    normal: UnitOwnerMultiRootNormalTransportV1,
    tail: Tail,
    entry: Entry,
    exit: (Vec<UnitSymbolId>, Vec<UnitDropFact>),
    suffix: Option<Vec<(UnitSymbolId, UnitDropFact)>>,
}
impl PrivateChainMember {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        sites: (UnitStatementId, UnitExpressionId, UnitStatementId),
        normal: UnitOwnerMultiRootNormalTransportV1,
        tail: Tail,
        entry: Entry,
        exit: (Vec<UnitSymbolId>, Vec<UnitDropFact>),
        suffix: Option<Vec<(UnitSymbolId, UnitDropFact)>>,
    ) -> Option<Self> {
        let (control, condition, body) = sites;
        let member = Self {
            control,
            condition,
            body,
            normal,
            tail,
            entry,
            exit,
            suffix,
        };
        member.is_consistent().then_some(member)
    }
    fn is_consistent(&self) -> bool {
        let roots = self.normal.header_roots();
        let (live, drops) = &self.exit;
        let source = self.control.source_unit();
        if !self.normal.is_consistent()
            || self.normal.statement() != self.control
            || self.body == self.control
            || self.body.source_unit() != source
            || self.condition.source_unit() != source
            || self.tail.0.source_unit() != source
            || self.tail.1.source_unit() != source
            || self.entry.0.iter().any(|s| s.source_unit() != source)
            || self.tail.0 == self.tail.1
            || roots.contains(&self.tail.0)
            || roots.contains(&self.tail.1)
            || self.entry.0 != [self.tail.1, self.tail.0]
            || self.entry.1[0] == self.entry.1[1]
            || self.entry.2[0] == self.entry.2[1]
            || self
                .tail
                .2
                .iter()
                .chain(self.entry.1.iter())
                .any(|e| e.source_unit() != source)
            || self.entry.2.iter().any(|s| s.source_unit() != source)
            || live.iter().any(|r| !roots.contains(r))
            || drops.iter().any(|d| {
                d.point() != UnitDropPoint::LoopExit(self.control)
                    || !matches!(d.target(), UnitDropTarget::Named(r) if roots.contains(&r))
            })
            || roots.iter().any(|r| {
                live.iter().filter(|s| *s == r).count()
                    + drops
                        .iter()
                        .filter(|d| d.target() == UnitDropTarget::Named(*r))
                        .count()
                    != 1
            })
        {
            return false;
        }
        if live.is_empty() {
            return self.suffix.is_none();
        }
        let Some(suffix) = &self.suffix else {
            return false;
        };
        suffix.len() == live.len() && live.iter().all(|r| suffix.iter().filter(|(s,_)| s == r).count() == 1)
            && suffix.iter().enumerate().all(|(i,(root,d))| live.contains(root)
                && matches!(d.target(), UnitDropTarget::Named(binding) if binding.source_unit() == source && !roots.contains(&binding))
                && matches!(d.point(), UnitDropPoint::AfterStatement(s) if s.source_unit() == source)
                && suffix[..i].iter().all(|(_,prior)| prior.target() != d.target() && prior.point() != d.point()))
    }
}
#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct PrivateControlChain {
    first: UnitOwnerControlBridgeEvidenceV1,
    predecessor: UnitOwnerMultiRootNormalTransportV1,
    sources: Vec<UnitSymbolId>,
    targets: Vec<UnitSymbolId>,
    last: PrivateChainMember,
    leading_entry: Option<Entry>,
}
impl PrivateControlChain {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        first: UnitOwnerControlBridgeEvidenceV1,
        predecessor: UnitOwnerMultiRootNormalTransportV1,
        sources: Vec<UnitSymbolId>,
        targets: Vec<UnitSymbolId>,
        last: PrivateChainMember,
        leading_entry: Option<Entry>,
    ) -> Option<Self> {
        let group = Self {
            first,
            predecessor,
            sources,
            targets,
            last,
            leading_entry,
        };
        group.is_consistent().then_some(group)
    }
    pub(in crate::ownership_checking::compilation_unit) fn is_consistent(&self) -> bool {
        let Some(normal) = self.first.next_normal_transport() else {
            return false;
        };
        let Some((live, _)) = self.first.next_false_exit_partition() else {
            return false;
        };
        self.leading_entry.is_none_or(|e| {
            let p = self.first.prior();
            let source = p.statement().source_unit();
            e.0 == [p.counter(), p.guard()]
                && e.1[1] == p.initializer()
                && e.1[0] != e.1[1]
                && e.2[0] != e.2[1]
                && e.1.iter().all(|id| id.source_unit() == source)
                && e.2.iter().all(|id| id.source_unit() == source)
        }) && self.first.is_consistent()
            && self.first.next_counter_tail().is_some()
            && self.first.next_counter_entry().is_some()
            && self.predecessor.is_consistent()
            && self.predecessor.statement() == normal.statement()
            && self.predecessor.header_roots() == normal.header_roots()
            && self.predecessor.steps() == normal.steps()
            && Arc::ptr_eq(
                &self.predecessor.body_catalog().identity,
                &normal.body_catalog().identity,
            )
            && self.sources
                == normal
                    .header_roots()
                    .iter()
                    .filter(|r| live.contains(r))
                    .copied()
                    .collect::<Vec<_>>()
            && self.sources.len() >= 2
            && self.sources.len() == self.targets.len()
            && self
                .targets
                .iter()
                .enumerate()
                .all(|(i, r)| !normal.header_roots().contains(r) && !self.targets[..i].contains(r))
            && self.last.is_consistent()
            && self.last.normal.header_roots() == self.targets
            && self.last.control != self.first.prior().statement()
            && self.last.control != self.first.next_control()
            && self.last.control.source_unit() == self.first.next_control().source_unit()
            && self.last.normal.body_catalog().callable() == self.first.prior().callable()
            && !Arc::ptr_eq(
                &self.last.normal.body_catalog().identity,
                &normal.body_catalog().identity,
            )
    }
    pub(in crate::ownership_checking::compilation_unit) fn matches_observation(
        &self,
        identity: &Arc<()>,
        callable: UnitOwnerCallable,
    ) -> bool {
        self.is_consistent()
            && Arc::ptr_eq(identity, &self.first.prior().analysis)
            && callable == self.first.prior().callable()
    }
    pub(in crate::ownership_checking::compilation_unit) fn first_control(&self) -> UnitStatementId {
        self.first.prior().statement()
    }
}

/// One member of an indivisible normal-body and implicit-false partition group.
/// This carries no concrete incoming owner or whole-loop permission.
#[derive(Clone, Copy, Debug)]
pub struct UnitOwnerControlChainBodyV1<'a> {
    control: UnitStatementId,
    condition: UnitExpressionId,
    body: UnitStatementId,
    normal: &'a UnitOwnerMultiRootNormalTransportV1,
    false_exit: (&'a [UnitSymbolId], &'a [UnitDropFact]),
    tail: Tail,
    suffix: Option<&'a [(UnitSymbolId, UnitDropFact)]>,
    entry: Entry,
    prior_control: UnitStatementId,
}
impl<'a> UnitOwnerControlChainBodyV1<'a> {
    /// Actual source-qualified control owning this body.
    pub fn control(&self) -> UnitStatementId {
        self.control
    }
    /// Actual completed condition associated with this member.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual source body entered by this control.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Borrow the original normal-body catalog and ordered retirement steps.
    pub fn normal(&self) -> &'a UnitOwnerMultiRootNormalTransportV1 {
        self.normal
    }
    /// Actual implicit-false live roots and retired-root cleanup facts.
    /// Facts retain their original order and LoopExit control; no suffix permission.
    pub fn false_exit_partition(&self) -> (&'a [UnitSymbolId], &'a [UnitDropFact]) {
        self.false_exit
    }
    /// Completed counter, guard and ordered decrement/arithmetic/reset/comparison sites.
    /// These are source facts, not an initializer or dynamic iteration proof.
    pub fn checked_tail(&self) -> (UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4]) {
        (self.tail.1, self.tail.0, self.tail.2)
    }
    /// Original final straight-line live-root binding and cleanup facts, if present.
    pub fn false_exit_suffix(&self) -> Option<&'a [(UnitSymbolId, UnitDropFact)]> {
        self.suffix
    }
    /// Original completed counter and guard initializer/declaration identities.
    pub fn scalar_entry(
        &self,
    ) -> (
        [UnitSymbolId; 2],
        [UnitExpressionId; 2],
        [UnitStatementId; 2],
    ) {
        self.entry
    }
    /// Actual immediately preceding control whose exit contains the initializers.
    pub fn prior_control(&self) -> UnitStatementId {
        self.prior_control
    }
    /// Source-root order, not the order of alias declarations or replacement calls.
    pub fn roots(&self) -> &'a [UnitSymbolId] {
        self.normal.header_roots()
    }
}
/// Original leading-control geometry and completed root mapping; no entry-token authority.
#[derive(Clone, Copy, Debug)]
pub struct UnitOwnerControlChainLeadingV1<'a> {
    control: UnitStatementId,
    condition: UnitExpressionId,
    body: UnitStatementId,
    sources: &'a [UnitSymbolId],
    targets: &'a [UnitSymbolId],
    normal: &'a UnitOwnerMultiRootNormalTransportV1,
    tail: Tail,
    entry: Option<Entry>,
    false_exit: (&'a [UnitSymbolId], &'a [UnitDropFact]),
}
impl<'a> UnitOwnerControlChainLeadingV1<'a> {
    /// Actual first control, not a synthesized group member.
    pub fn control(&self) -> UnitStatementId {
        self.control
    }
    /// Actual first control condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual first control body.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Original first-body normal rule, including every body root and actual producer catalog.
    /// This grants no first scalar-entry, checked-tail or false-exit contract.
    pub fn normal(&self) -> &'a UnitOwnerMultiRootNormalTransportV1 {
        self.normal
    }
    /// Actual completed first counter, guard and decrement/arithmetic/reset/comparison sites.
    /// Consumers must validate checked success/abort; this is not scalar initialization evidence.
    pub fn checked_tail(&self) -> (UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4]) {
        (self.tail.1, self.tail.0, self.tail.2)
    }
    /// Actual ordered scalar initializer completions, frozen at the first while entry.
    /// Absent when the restricted Name/Integer initialization has no complete event pair.
    pub fn scalar_entry(
        &self,
    ) -> Option<(
        [UnitSymbolId; 2],
        [UnitExpressionId; 2],
        [UnitStatementId; 2],
    )> {
        self.entry
    }
    /// Actual implicit-false live roots and retirement facts, not suffix permission.
    pub fn false_exit_partition(&self) -> (&'a [UnitSymbolId], &'a [UnitDropFact]) {
        self.false_exit
    }
    /// Completed source-to-target root mapping of the first link.
    pub fn roots(&self) -> (&'a [UnitSymbolId], &'a [UnitSymbolId]) {
        (self.sources, self.targets)
    }
}
/// The second and third bodies of one validated chain, in control order.
/// Consumers must select and complete both members atomically, never fall back
/// to independently authorizing one member. No entry token is synthesized.
#[derive(Debug)]
pub struct UnitOwnerControlChainBodiesV1<'a> {
    members: [UnitOwnerControlChainBodyV1<'a>; 2],
    link: (&'a [UnitSymbolId], &'a [UnitSymbolId]),
    leading: UnitOwnerControlChainLeadingV1<'a>,
}
impl<'a> UnitOwnerControlChainBodiesV1<'a> {
    /// Borrow the first link geometry/mapping without granting its body permission.
    pub fn leading(&self) -> UnitOwnerControlChainLeadingV1<'a> {
        self.leading
    }
    /// Original completed second-to-third source/target mapping, including sparse roots.
    pub fn inter_member_roots(&self) -> (&'a [UnitSymbolId], &'a [UnitSymbolId]) {
        self.link
    }
    /// Both continuation bodies in control order; consume as one group.
    pub fn members(&self) -> &[UnitOwnerControlChainBodyV1<'a>; 2] {
        &self.members
    }
}
impl UnitOwnerObservation {
    /// Borrow normal-body rules and implicit-false partitions of a complete chain.
    /// Consumers must validate actual checked edges; no whole-chain SSA permission.
    pub fn control_chain_bodies_v1(&self) -> Option<UnitOwnerControlChainBodiesV1<'_>> {
        if !self.is_consistent() {
            return None;
        }
        let chain = self.private_control_chain.as_ref()?;
        let second = UnitOwnerControlChainBodyV1 {
            control: chain.first.next_control(),
            condition: chain.first.next_condition(),
            body: chain.first.next_body(),
            normal: chain.first.next_normal_transport()?,
            false_exit: chain.first.next_false_exit_partition()?,
            suffix: None,
            prior_control: chain.first.prior().statement(),
            entry: {
                let e = chain.first.next_counter_entry()?;
                ([e.counter(), e.guard()], e.initializers(), e.declarations())
            },
            tail: {
                let tail = chain.first.next_counter_tail()?;
                (tail.guard(), tail.counter(), tail.assignments())
            },
        };
        let third = UnitOwnerControlChainBodyV1 {
            control: chain.last.control,
            condition: chain.last.condition,
            body: chain.last.body,
            normal: &chain.last.normal,
            false_exit: (&chain.last.exit.0, &chain.last.exit.1),
            tail: chain.last.tail,
            suffix: chain.last.suffix.as_deref(),
            entry: chain.last.entry,
            prior_control: chain.first.next_control(),
        };
        Some(UnitOwnerControlChainBodiesV1 {
            members: [second, third],
            link: (&chain.sources, &chain.targets),
            leading: UnitOwnerControlChainLeadingV1 {
                control: chain.first.prior().statement(),
                entry: chain.leading_entry,
                condition: chain.first.prior().condition(),
                body: chain.first.prior().body(),
                sources: chain.first.sources(),
                targets: chain.first.targets(),
                normal: chain.first.prior().roots.normal_transport()?,
                false_exit: chain.first.prior().false_exit_partition()?,
                tail: (
                    chain.first.prior().guard(),
                    chain.first.prior().counter(),
                    chain.first.prior().assignments(),
                ),
            },
        })
    }
}
