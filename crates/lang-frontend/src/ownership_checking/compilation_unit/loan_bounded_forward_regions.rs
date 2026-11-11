//! Bounded dynamic terminal tree evidence, independent of fixed topology certificates.
use super::{UnitLoanActionCoverage, UnitLoanActionId, UnitLoanNestedForwardEdge};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;
/// Actual acquisition-to-handoff lineage. This is not a region certificate or receipt authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBoundedBorrowForwardPath {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) control: UnitExpressionId,
    pub(super) value: UnitExpressionId,
    pub(super) begin: usize,
    pub(super) handoff: usize,
    pub(super) edges: Vec<UnitLoanNestedForwardEdge>,
}
impl UnitBoundedBorrowForwardPath {
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

/// Actual complete closure of a bounded terminal tree, never whole-function or backend authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBoundedBorrowForwardRegion {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) split_count: usize,
    pub(super) paths: Vec<UnitBoundedBorrowForwardPath>,
}
impl UnitBoundedBorrowForwardRegion {
    /// Source-qualified wrapper defining this tree.
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// Single non-owning source parameter.
    pub const fn source_parameter(&self) -> UnitSymbolId {
        self.root
    }
    /// Independent closure coverage of only this bounded tree.
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }
    /// Number of distinct, actually traversed conditional splits in the declared tree.
    pub const fn split_count(&self) -> usize {
        self.split_count
    }
    /// Actual DFS acquisition lineages, empty unless the whole bounded tree closes.
    pub fn paths(&self) -> &[UnitBoundedBorrowForwardPath] {
        &self.paths
    }
}

/// Actual single-survivor chain, separate from terminal child closure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBoundedBorrowContinuation {
    pub(super) entries: Vec<UnitLoanNestedForwardEdge>,
    pub(super) resumptions: Vec<UnitLoanNestedForwardEdge>,
    pub(super) tail_control: UnitExpressionId,
}
impl UnitBoundedBorrowContinuation {
    /// Actual hole provenance, ordered from root to the implicit false entry.
    pub fn entries(&self) -> &[UnitLoanNestedForwardEdge] {
        &self.entries
    }
    /// Consumed survivor tokens, ordered from the innermost join to the root join.
    pub fn resumptions(&self) -> &[UnitLoanNestedForwardEdge] {
        &self.resumptions
    }
    /// The unique return executed after root continuation resumes.
    pub const fn tail_control(&self) -> UnitExpressionId {
        self.tail_control
    }
}
/// Independently closed bounded single-continuation tree; not terminal-tree receipt authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBoundedBorrowContinuationRegion {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) split_count: usize,
    pub(super) paths: Vec<UnitBoundedBorrowForwardPath>,
    pub(super) continuation: Option<UnitBoundedBorrowContinuation>,
}
impl UnitBoundedBorrowContinuationRegion {
    /// Exact wrapper declaration owning this certificate.
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// Single borrowed source parameter.
    pub const fn source_parameter(&self) -> UnitSymbolId {
        self.root
    }
    /// Coverage of only this independently closed tree and its tail.
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }
    /// Distinct actually entered splits.
    pub const fn split_count(&self) -> usize {
        self.split_count
    }
    /// Actual return order, with the resumed tail last; empty unless Complete.
    pub fn paths(&self) -> &[UnitBoundedBorrowForwardPath] {
        &self.paths
    }
    /// Actual survivor chain; absent unless every join, resume, tail and close was proved.
    pub fn continuation(&self) -> Option<&UnitBoundedBorrowContinuation> {
        self.continuation.as_ref()
    }
}
