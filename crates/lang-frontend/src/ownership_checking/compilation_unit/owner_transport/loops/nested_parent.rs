//! Observed parent boundaries, not a generational induction or lowering permission.
use super::*;
use crate::ownership_checking::compilation_unit::UnitSymbolId;
use crate::ownership_checking::{UnitDropFact, UnitDropTarget};
/// Independently observed direct-parent root boundaries; no new lowering capability.
#[derive(Clone, Debug)]
pub struct UnitOwnerNestedParentBoundaryV1 {
    sites: (UnitStatementId, UnitExpressionId, UnitStatementId),
    roots: Vec<UnitSymbolId>,
    partition: (Vec<UnitSymbolId>, Vec<UnitDropFact>),
}
impl UnitOwnerNestedParentBoundaryV1 {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        sites: (UnitStatementId, UnitExpressionId, UnitStatementId),
        roots: Vec<UnitSymbolId>,
        partition: (Vec<UnitSymbolId>, Vec<UnitDropFact>),
    ) -> Option<Self> {
        let result = Self {
            sites,
            roots,
            partition,
        };
        result.is_consistent().then_some(result)
    }
    /// Actual parent While.
    pub fn control(&self) -> UnitStatementId {
        self.sites.0
    }
    /// Actual parent condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.sites.1
    }
    /// Actual parent body.
    pub fn body(&self) -> UnitStatementId {
        self.sites.2
    }
    /// Complete root identities observed at entry, condition, body entry and normal backedge.
    pub fn roots(&self) -> &[UnitSymbolId] {
        &self.roots
    }
    /// Fresh parent LoopExit cleanup and surviving roots; not child cleanup.
    pub fn false_exit_partition(&self) -> (&[UnitSymbolId], &[UnitDropFact]) {
        (&self.partition.0, &self.partition.1)
    }
    /// Check local identities and the exact parent false cleanup partition.
    pub fn is_consistent(&self) -> bool {
        let (live, drops) = &self.partition;
        self.control() != self.body()
            && self.condition().source_unit() == self.control().source_unit()
            && self.body().source_unit() == self.control().source_unit()
            && self.roots.len() >= 2
            && self.roots.iter().enumerate().all(|(i, r)| {
                r.source_unit() == self.control().source_unit() && !self.roots[..i].contains(r)
            })
            && live.len() + drops.len() == self.roots.len()
            && live
                .iter()
                .enumerate()
                .all(|(i, r)| self.roots.contains(r) && !live[..i].contains(r))
            && self.roots.iter().all(|r| {
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
}
