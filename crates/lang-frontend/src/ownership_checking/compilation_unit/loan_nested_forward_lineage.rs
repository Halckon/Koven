//! Partial provenance of an actually entered nested forwarding leaf, never path completeness.
use super::{UnitLoanActionCoverage, UnitLoanActionId};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;

/// An observed ancestor branch entry, ordered from outermost to innermost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitLoanNestedForwardEdge {
    pub(super) split: UnitExpressionId,
    pub(super) condition: UnitExpressionId,
    pub(super) then_arm: bool,
}
impl UnitLoanNestedForwardEdge {
    /// Original If expression whose arm was actually entered.
    pub const fn split(&self) -> UnitExpressionId {
        self.split
    }
    /// Original condition evaluated before this entry.
    pub const fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Whether the actual entry selected the then arm.
    pub const fn then_arm(&self) -> bool {
        self.then_arm
    }
}

/// Actual acquisition-to-handoff lineage. This is not a region certificate or receipt authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanNestedForwardLineage {
    pub(super) topology: super::UnitBorrowNestedForwardTopology,
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) control: UnitExpressionId,
    pub(super) value: UnitExpressionId,
    pub(super) begin: usize,
    pub(super) handoff: usize,
    pub(super) edges: Vec<UnitLoanNestedForwardEdge>,
}
impl UnitLoanNestedForwardLineage {
    /// Exact admitted nesting side, bound to the actual parent entry.
    pub const fn topology(&self) -> super::UnitBorrowNestedForwardTopology {
        self.topology
    }
    /// Permanently partial: no all-path, cleanup, or backend admission claim.
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        UnitLoanActionCoverage::Partial
    }
    /// Source-qualified wrapper owning this observed path.
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// Non-owning parameter supplying the live forwarded loan.
    pub const fn source_parameter(&self) -> UnitSymbolId {
        self.root
    }
    /// Actual Return expression, distinct from its returned call.
    pub const fn control(&self) -> UnitExpressionId {
        self.control
    }
    /// Actual returned call whose source acquisition was handed off.
    pub const fn value(&self) -> UnitExpressionId {
        self.value
    }
    /// Catalog-bound identity of the actual source acquisition.
    pub fn source_begin(&self) -> UnitLoanActionId {
        UnitLoanActionId {
            identity: self.identity.clone(),
            index: self.begin,
        }
    }
    /// Catalog-bound identity of its actual live delivery, not a loan end.
    pub fn handoff(&self) -> UnitLoanActionId {
        UnitLoanActionId {
            identity: self.identity.clone(),
            index: self.handoff,
        }
    }
    /// Validated actual ancestor entries, ordered outermost to innermost.
    pub fn edges(&self) -> &[UnitLoanNestedForwardEdge] {
        &self.edges
    }
}
