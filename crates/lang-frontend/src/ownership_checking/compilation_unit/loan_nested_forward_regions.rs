//! Independent fixed three- or four-leaf closure certificate; never whole-callable or backend authority.
use super::{UnitLoanActionCoverage, UnitLoanNestedForwardLineage};
use crate::{name_resolution::UnitSymbolId, type_checking::UnitItemId};
use std::sync::Arc;

/// The exact admitted finite nested tree, never arbitrary nesting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitBorrowNestedForwardTopology {
    /// The outer true arm contains the inner split.
    NestedThen,
    /// The outer false arm contains the inner split.
    NestedElse,
    /// Both outer arms contain one complete inner split, exactly four leaves.
    BalancedBoth,
}

/// Separately certifies actual terminal closure of the admitted fixed nested region.
/// Its constituent acquisition lineages retain their original Partial meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowNestedForwardRegion {
    pub(super) topology: UnitBorrowNestedForwardTopology,
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) paths: Vec<UnitLoanNestedForwardLineage>,
}
impl UnitBorrowNestedForwardRegion {
    /// The positively admitted finite nested tree topology.
    pub const fn topology(&self) -> UnitBorrowNestedForwardTopology {
        self.topology
    }
    /// The original function defining this bounded region.
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// The one shared non-owning source parameter.
    pub const fn source_parameter(&self) -> UnitSymbolId {
        self.root
    }
    /// Closure coverage of only this admitted region; not backend admission.
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }
    /// Three or four actual root-to-leaf lineages for Complete, otherwise empty.
    /// Enumeration is not runtime sequencing between mutually exclusive paths.
    pub fn paths(&self) -> &[UnitLoanNestedForwardLineage] {
        &self.paths
    }
}

impl UnitBorrowNestedForwardTopology {
    pub(crate) const fn nested_then(self) -> bool {
        matches!(self, Self::NestedThen)
    }
    pub(crate) const fn leaf_path(self, leaf: usize) -> [Option<(usize, bool)>; 2] {
        match (self, leaf) {
            (Self::NestedThen, 0) => [Some((0, true)), Some((1, true))],
            (Self::NestedThen, 1) => [Some((0, true)), Some((1, false))],
            (Self::NestedThen, 2) => [Some((0, false)), None],
            (Self::NestedElse, 0) => [Some((0, true)), None],
            (Self::NestedElse, 1) => [Some((0, false)), Some((1, true))],
            (Self::NestedElse, 2) => [Some((0, false)), Some((1, false))],
            (Self::BalancedBoth, 0) => [Some((0, true)), Some((1, true))],
            (Self::BalancedBoth, 1) => [Some((0, true)), Some((1, false))],
            (Self::BalancedBoth, 2) => [Some((0, false)), Some((2, true))],
            (Self::BalancedBoth, 3) => [Some((0, false)), Some((2, false))],
            _ => [None, None],
        }
    }
}
