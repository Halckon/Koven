//! Independently certified paths of a narrow normal branch-forward region.
use super::{UnitLoanActionCoverage, UnitLoanActionId, UnitLoanForwardBranch};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::{ops::Range, sync::Arc};

/// Exact control topology certified by the two-path region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitBorrowBranchForwardTopology {
    /// A terminating then arm and a false-edge continuation to the tail return.
    TailContinuation,
    /// Both explicit arms return; no live continuation exists.
    BothArmsReturn,
}

/// Independently scoped branch coverage, never a linear sequence across both arms.
/// Source/callable coverage and backend
/// receipt eligibility are unaffected by this separate product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowBranchForwardRegion {
    pub(super) topology: UnitBorrowBranchForwardTopology,
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) parameter: UnitSymbolId,
    pub(super) split: UnitExpressionId,
    pub(super) condition: UnitExpressionId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) paths: Vec<UnitBorrowBranchForwardPath>,
}

impl UnitBorrowBranchForwardRegion {
    /// Positively witnessed topology, not inferred from optional records.
    pub const fn topology(&self) -> UnitBorrowBranchForwardTopology {
        self.topology
    }
    /// Original wrapper owning the declared region.
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// Its non-owning source parameter, not a caller-local loan identity.
    pub const fn source_parameter(&self) -> UnitSymbolId {
        self.parameter
    }
    /// Original dynamic split.
    pub const fn split(&self) -> UnitExpressionId {
        self.split
    }
    /// Original Boolean parameter read.
    pub const fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Exactly two certified paths for Complete; empty for Partial.
    /// Enumeration order is not runtime execution order across mutually exclusive arms.
    pub fn paths(&self) -> &[UnitBorrowBranchForwardPath] {
        &self.paths
    }
    /// Coverage of the explicitly declared region, not the whole source or callable.
    #[must_use]
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }
}

/// One actual arm entry through its completed live Return, never a cross-arm interval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowBranchForwardPath {
    pub(super) identity: Arc<()>,
    pub(super) branch: UnitLoanForwardBranch,
    pub(super) records: Range<usize>,
}
impl UnitBorrowBranchForwardPath {
    /// Exact original control/value and arm identity.
    pub const fn branch(&self) -> UnitLoanForwardBranch {
        self.branch
    }
    /// Actual acquisition in this catalog.
    pub fn source_begin(&self) -> UnitLoanActionId {
        self.id(self.records.start)
    }
    /// Actual live delivery; it is not End or a backend receipt.
    pub fn live_handoff(&self) -> UnitLoanActionId {
        self.id(self.records.end - 1)
    }
    /// Every original record in this path, without role filtering.
    pub fn action_ids(&self) -> impl ExactSizeIterator<Item = UnitLoanActionId> + '_ {
        self.records.clone().map(|index| self.id(index))
    }
    fn id(&self, index: usize) -> UnitLoanActionId {
        UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        }
    }
}
