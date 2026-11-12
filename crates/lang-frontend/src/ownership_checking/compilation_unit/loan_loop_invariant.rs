//! Actual planner boundaries for an outer binding carried through a normal while body.
use super::{UnitBindingUseReturn, UnitLoanActionCoverage, UnitLoanActionId};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitStatementId},
};
use std::sync::Arc;
/// Partial boundary lineage, not a fixed-point, per-iteration, or cleanup certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanLoopInvariantLineage {
    pub(super) identity: Arc<()>,
    pub(super) binding: UnitSymbolId,
    pub(super) statement: UnitStatementId,
    pub(super) condition: UnitExpressionId,
    pub(super) body: UnitStatementId,
    pub(super) source_begin: UnitLoanActionId,
    pub(super) handoff: UnitLoanActionId,
    pub(super) binding_end: UnitLoanActionId,
    pub(super) uses: Vec<UnitBindingUseReturn>,
    pub(super) cursors: [usize; 4],
}
impl UnitLoanLoopInvariantLineage {
    /// No arbitrary-iteration completeness or source-drop authority is granted.
    pub fn coverage(&self) -> UnitLoanActionCoverage {
        UnitLoanActionCoverage::Partial
    }
    /// Actual outer result binding.
    pub fn binding(&self) -> UnitSymbolId {
        self.binding
    }
    /// Observed while statement.
    pub fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// Actual normally evaluated condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Body whose normal planner traversal returned with the same lease.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Original evaluated source acquisition.
    pub fn source_begin(&self) -> UnitLoanActionId {
        self.source_begin.clone()
    }
    /// Original initializer commit.
    pub fn handoff(&self) -> UnitLoanActionId {
        self.handoff.clone()
    }
    /// Actual outer binding removal, after the normal loop exit.
    pub fn binding_end(&self) -> UnitLoanActionId {
        self.binding_end.clone()
    }
    /// Actual consumer pending tokens removed on normal return during the observed body traversal.
    pub fn uses(&self) -> &[UnitBindingUseReturn] {
        &self.uses
    }
    /// Original record boundaries: condition return, body entry, body return, normal exit.
    /// These are traversal boundaries, not a runtime iteration count.
    pub fn boundary_cursors(&self) -> [usize; 4] {
        self.cursors
    }
}
