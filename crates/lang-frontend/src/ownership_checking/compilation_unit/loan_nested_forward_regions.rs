//! Independent fixed three- or four-leaf closure certificate; never whole-callable or backend authority.
use super::{UnitLoanActionCoverage, UnitLoanNestedForwardLineage};
use crate::{name_resolution::UnitSymbolId, type_checking::UnitItemId};
use std::sync::Arc;

/// The exact admitted finite nested tree, never arbitrary nesting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitBorrowNestedForwardTopology {
    /// The outer true arm contains the inner split.
    NestedThen,
    /// A complete inner true subtree followed by one real outer-false tail continuation.
    NestedThenTail,
    /// The outer false arm contains the inner split.
    NestedElse,
    /// Both outer arms contain one complete inner split, exactly four leaves.
    BalancedBoth,
}

/// Exact parameter-to-condition identity layout, independent of control-point identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitBorrowNestedForwardFlagLayout {
    /// Every condition reads its own distinct Boolean parameter.
    Distinct,
    /// Only the two balanced inner conditions share one Boolean parameter.
    SharedInner,
}

/// Actual unique outer-false continuation, validated before and after the real state merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowNestedForwardContinuation {
    pub(super) entry: super::UnitLoanNestedForwardEdge,
    pub(super) split: crate::type_checking::UnitExpressionId,
    pub(super) condition: crate::type_checking::UnitExpressionId,
    pub(super) tail_control: crate::type_checking::UnitExpressionId,
}
impl UnitBorrowNestedForwardContinuation {
    /// Actual observed outer-false entry which supplied the unique surviving state.
    pub const fn entry(&self) -> &super::UnitLoanNestedForwardEdge {
        &self.entry
    }

    /// Original outer split whose true subtree terminated.
    pub const fn split(&self) -> crate::type_checking::UnitExpressionId {
        self.split
    }
    /// Actual dynamic condition selecting the surviving false edge.
    pub const fn condition(&self) -> crate::type_checking::UnitExpressionId {
        self.condition
    }
    /// Explicit tail return reached after the verified continuation.
    pub const fn tail_control(&self) -> crate::type_checking::UnitExpressionId {
        self.tail_control
    }
}

/// Separately certifies actual terminal closure of the admitted fixed nested region.
/// Its constituent acquisition lineages retain their original Partial meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowNestedForwardRegion {
    pub(super) continuation: Option<UnitBorrowNestedForwardContinuation>,
    pub(super) flag_layout: UnitBorrowNestedForwardFlagLayout,
    pub(super) topology: UnitBorrowNestedForwardTopology,
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) paths: Vec<UnitLoanNestedForwardLineage>,
}
impl UnitBorrowNestedForwardRegion {
    /// Actual continuation only for a Complete NestedThenTail region; absent on other trees.
    pub const fn continuation(&self) -> Option<&UnitBorrowNestedForwardContinuation> {
        self.continuation.as_ref()
    }

    /// Exact admitted parameter identity layout; never merges independent control points.
    pub const fn flag_layout(&self) -> UnitBorrowNestedForwardFlagLayout {
        self.flag_layout
    }

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
        matches!(self, Self::NestedThen | Self::NestedThenTail)
    }
    pub(crate) const fn leaf_path(self, leaf: usize) -> [Option<(usize, bool)>; 2] {
        match (self, leaf) {
            (Self::NestedThen | Self::NestedThenTail, 0) => [Some((0, true)), Some((1, true))],
            (Self::NestedThen | Self::NestedThenTail, 1) => [Some((0, true)), Some((1, false))],
            (Self::NestedThen | Self::NestedThenTail, 2) => [Some((0, false)), None],
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
