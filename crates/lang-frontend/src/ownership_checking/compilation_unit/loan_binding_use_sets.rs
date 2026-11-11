//! Independent normal-return collection; never a retirement or owner-destruction certificate.
use super::{UnitDropPoint, UnitLoanActionCoverage, UnitLoanActionId, UnitLoanFact};
use crate::{name_resolution::UnitSymbolId, type_checking::UnitItemId};
use std::sync::Arc;
/// One actual binding-consumer pending token removed on normal return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBindingUseReturn {
    pub(super) loan: UnitLoanFact,
    pub(super) begin: UnitLoanActionId,
    pub(super) cursor: usize,
}
impl UnitBindingUseReturn {
    /// Actual removed pending loan, not a new backend loan allocation.
    pub fn loan(&self) -> &UnitLoanFact {
        &self.loan
    }
    /// Original acquisition observation identity.
    pub fn begin(&self) -> UnitLoanActionId {
        self.begin.clone()
    }
    /// Normal-return point observed by pending-token removal.
    pub fn return_point(&self) -> UnitDropPoint {
        UnitDropPoint::CallReturn(self.loan.call())
    }
    /// Original action boundary at return; not an End action ID.
    pub fn return_cursor(&self) -> usize {
        self.cursor
    }
}
/// A bounded exact set of actual binding uses closed by actual BindingEnd.
/// Coverage is always Partial: no source-retirement, owner-Drop or full-region authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingUseSet {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) binding: UnitSymbolId,
    pub(super) source_begin: UnitLoanActionId,
    pub(super) handoff: UnitLoanActionId,
    pub(super) binding_end: UnitLoanActionId,
    pub(super) uses: Vec<UnitBindingUseReturn>,
}
impl UnitLoanBindingUseSet {
    /// Containing function.
    pub fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// The one initialized result binding.
    pub fn binding(&self) -> UnitSymbolId {
        self.binding
    }
    /// This product is deliberately not a complete lifecycle proof.
    pub fn coverage(&self) -> UnitLoanActionCoverage {
        UnitLoanActionCoverage::Partial
    }
    /// Original source acquisition.
    pub fn source_begin(&self) -> UnitLoanActionId {
        self.source_begin.clone()
    }
    /// Original committed initializer handoff.
    pub fn handoff(&self) -> UnitLoanActionId {
        self.handoff.clone()
    }
    /// Actual end emitted after removing the initialized binding and its transport.
    pub fn binding_end(&self) -> UnitLoanActionId {
        self.binding_end.clone()
    }
    /// Exact actual normal returns, in declaration and execution order.
    pub fn uses(&self) -> &[UnitBindingUseReturn] {
        &self.uses
    }
}

/// Independent consumption of an exact use set followed by actual source-owner destruction.
/// This remains Partial: it does not certify the auxiliary lifecycle or an entire region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingUseSetRetirement {
    pub(super) use_set: UnitLoanBindingUseSet,
    pub(super) source_owner: super::UnitOwnerId,
    pub(super) source_drop: UnitLoanActionId,
}
impl UnitLoanBindingUseSetRetirement {
    /// Original validated partial use collection, including its actual BindingEnd.
    pub fn use_set(&self) -> &UnitLoanBindingUseSet {
        &self.use_set
    }
    /// Actual source owner removed by the witnessed cleanup attempt.
    pub fn source_owner(&self) -> &super::UnitOwnerId {
        &self.source_owner
    }
    /// Same-catalog original cleanup Drop record.
    pub fn source_drop(&self) -> UnitLoanActionId {
        self.source_drop.clone()
    }
    /// No full region or auxiliary-lifecycle authority is granted.
    pub fn coverage(&self) -> UnitLoanActionCoverage {
        UnitLoanActionCoverage::Partial
    }
}
