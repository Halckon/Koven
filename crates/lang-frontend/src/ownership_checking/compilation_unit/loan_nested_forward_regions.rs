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
    /// Inner-false continuation resumes through the outer false arm and then the function tail.
    NestedElseCascadeTail,
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

/// Actual unique false-edge continuation, validated before and after the real state merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowNestedForwardContinuation {
    pub(super) entry: super::UnitLoanNestedForwardEdge,
    pub(super) split: crate::type_checking::UnitExpressionId,
    pub(super) condition: crate::type_checking::UnitExpressionId,
    pub(super) tail_control: crate::type_checking::UnitExpressionId,
}
impl UnitBorrowNestedForwardContinuation {
    /// Actual observed false-edge entry which supplied the unique surviving state.
    pub const fn entry(&self) -> &super::UnitLoanNestedForwardEdge {
        &self.entry
    }

    /// Original split whose true arm terminated.
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

/// Ordered two-merge certificate: the outer continuation consumes the restored inner survivor.
/// Both steps refer to the same actual FF lineage, not merely two independently counted events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowNestedForwardCascadeContinuation {
    pub(super) inner: UnitBorrowNestedForwardContinuation,
    pub(super) outer: UnitBorrowNestedForwardContinuation,
}
impl UnitBorrowNestedForwardCascadeContinuation {
    /// First step: the unique inner-false survivor, validated across its actual merge.
    pub const fn inner(&self) -> &UnitBorrowNestedForwardContinuation {
        &self.inner
    }
    /// Second step: the outer merge consuming the first step's restored survivor exactly once.
    pub const fn outer(&self) -> &UnitBorrowNestedForwardContinuation {
        &self.outer
    }
}

/// Separately certifies actual terminal closure of the admitted fixed nested region.
/// Its constituent acquisition lineages retain their original Partial meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowNestedForwardRegion {
    pub(super) continuation: Option<UnitBorrowNestedForwardContinuation>,
    pub(super) cascade_continuation: Option<UnitBorrowNestedForwardCascadeContinuation>,
    pub(super) flag_layout: UnitBorrowNestedForwardFlagLayout,
    pub(super) topology: UnitBorrowNestedForwardTopology,
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) paths: Vec<UnitLoanNestedForwardLineage>,
}
impl UnitBorrowNestedForwardRegion {
    /// Ordered inner-then-outer resumption proof, only for a Complete NestedElseCascadeTail.
    pub const fn cascade_continuation(
        &self,
    ) -> Option<&UnitBorrowNestedForwardCascadeContinuation> {
        self.cascade_continuation.as_ref()
    }

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
}

#[cfg(test)]
mod route_tests;

mod route_contract;
