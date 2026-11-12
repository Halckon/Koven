//! Complete bounded straight-line multi-use intervals, separate from partial collections.
use super::{
    UnitLoanActionCoverage, UnitLoanActionId, UnitLoanAuxiliarySequenceUse,
    UnitLoanBindingUseSetRetirement, UnitLoanFact, UnitOwnerId,
};
use crate::type_checking::UnitStatementId;
/// Actual source obligation at one auxiliary normal return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitMultiBindingSourceState {
    /// The original owner and exact use-set transport remained live.
    Active {
        /// Actual live owner identity.
        owner: UnitOwnerId,
        /// Original source acquisition.
        source_begin: UnitLoanActionId,
        /// Original committed handoff.
        handoff: UnitLoanActionId,
    },
    /// The complete use set was consumed and the source actually destroyed.
    Retired {
        /// Actual binding termination, not a fabricated single-use retirement.
        binding_end: UnitLoanActionId,
        /// Actual source cleanup.
        drop: UnitLoanActionId,
    },
}
/// One actual auxiliary return together with its contemporaneous binding progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitMultiBindingAuxiliaryUse {
    pub(super) actual: UnitLoanAuxiliarySequenceUse,
    pub(super) returned_binding_uses: usize,
    pub(super) source_state: UnitMultiBindingSourceState,
}
impl UnitMultiBindingAuxiliaryUse {
    /// Actual removed loan.
    pub fn loan(&self) -> &UnitLoanFact {
        self.actual.loan()
    }
    /// Original acquisition.
    pub fn begin(&self) -> UnitLoanActionId {
        self.actual.begin()
    }
    /// Original normal-return cleanup.
    pub fn end(&self) -> UnitLoanActionId {
        self.actual.end()
    }
    /// Actual live auxiliary owner.
    pub fn owner(&self) -> &UnitOwnerId {
        self.actual.owner()
    }
    /// Number of binding normal returns already observed at this return.
    pub fn returned_binding_uses(&self) -> usize {
        self.returned_binding_uses
    }
    /// Actual active transport or consumed-source evidence.
    pub fn source_state(&self) -> &UnitMultiBindingSourceState {
        &self.source_state
    }
}
/// Complete entry-to-exit interval after the two owner constructions.
/// Does not broaden partial use-set products or ordinary language execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowMultiBindingRegion {
    pub(super) retirement: UnitLoanBindingUseSetRetirement,
    pub(super) uses: Vec<UnitMultiBindingAuxiliaryUse>,
    pub(super) auxiliary_drop: UnitLoanActionId,
    pub(super) ids: Vec<UnitLoanActionId>,
    pub(super) entry: UnitStatementId,
    pub(super) completion: UnitStatementId,
}
impl UnitBorrowMultiBindingRegion {
    /// Independently validated source consumption and destruction.
    pub fn retirement(&self) -> &UnitLoanBindingUseSetRetirement {
        &self.retirement
    }
    /// Actual auxiliary returns in execution order.
    pub fn auxiliary_uses(&self) -> &[UnitMultiBindingAuxiliaryUse] {
        &self.uses
    }
    /// Actual auxiliary destruction after its final normal return.
    pub fn auxiliary_drop(&self) -> UnitLoanActionId {
        self.auxiliary_drop.clone()
    }
    /// Every original action in the contiguous interval, without filtering.
    pub fn action_ids(&self) -> impl ExactSizeIterator<Item = UnitLoanActionId> + '_ {
        self.ids.iter().cloned()
    }
    /// Actual binding-entry statement after both owners are present.
    pub fn entry_statement(&self) -> UnitStatementId {
        self.entry
    }
    /// Actual final statement and cleanup boundary.
    pub fn completion_statement(&self) -> UnitStatementId {
        self.completion
    }
    /// Qualification applies only to this bounded interval.
    pub fn coverage(&self) -> UnitLoanActionCoverage {
        UnitLoanActionCoverage::Complete
    }
}
