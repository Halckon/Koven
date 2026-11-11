//! Independent bounded three-leaf closure certificate; never whole-callable or backend authority.
use super::{UnitLoanActionCoverage, UnitLoanNestedForwardLineage};
use crate::{name_resolution::UnitSymbolId, type_checking::UnitItemId};
use std::sync::Arc;

/// Separately certifies actual terminal closure of the admitted nested three-leaf region.
/// Its constituent acquisition lineages retain their original Partial meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowNestedForwardRegion {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) paths: Vec<UnitLoanNestedForwardLineage>,
}
impl UnitBorrowNestedForwardRegion {
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
    /// Three actual root-to-leaf lineages for Complete, otherwise empty.
    /// Enumeration is not runtime sequencing between mutually exclusive paths.
    pub fn paths(&self) -> &[UnitLoanNestedForwardLineage] {
        &self.paths
    }
}
