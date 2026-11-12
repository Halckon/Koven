//! Independent auxiliary uses surrounding the sole result-binding consumer.
use super::{
    UnitBorrowLaterAuxiliaryBindingRegion, UnitLoanActionCoverage, UnitLoanActionId,
    UnitLoanAuxiliarySequenceUse, UnitLoanFact, UnitOwnerId,
};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
/// Actual source obligation at a normal auxiliary return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitAuxiliarySourceState {
    /// The original source owner and unused binding transport were still live.
    BeforeBinding {
        /// Actual live source owner identity.
        owner: UnitOwnerId,
        /// Original pending source acquisition identity.
        source_begin: UnitLoanActionId,
        /// Original committed binding handoff identity.
        handoff: UnitLoanActionId,
    },
    /// The binding was consumed and its original source was actually removed.
    AfterBinding {
        /// Actual source retirement event.
        retirement: UnitLoanActionId,
        /// Actual source destruction event.
        drop: UnitLoanActionId,
    },
}
/// A real auxiliary acquisition/return with its contemporaneous source obligation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanInterleavedAuxiliaryUse {
    pub(super) actual: UnitLoanAuxiliarySequenceUse,
    pub(super) source_state: UnitAuxiliarySourceState,
}
impl UnitLoanInterleavedAuxiliaryUse {
    /// Actual pending loan consumed by normal return.
    pub fn loan(&self) -> &UnitLoanFact {
        self.actual.loan()
    }
    /// Same-catalog acquisition identity.
    pub fn begin(&self) -> UnitLoanActionId {
        self.actual.begin()
    }
    /// Same-catalog normal-return identity.
    pub fn end(&self) -> UnitLoanActionId {
        self.actual.end()
    }
    /// Actual live auxiliary owner identity.
    pub fn owner(&self) -> &UnitOwnerId {
        self.actual.owner()
    }
    /// Actual source obligation captured at this return.
    pub fn source_state(&self) -> &UnitAuxiliarySourceState {
        &self.source_state
    }
}
/// An independently qualified A*BA* interval with a bounded dynamic auxiliary-use sequence.
/// It begins after two owner constructions. Complete covers only this declared interval;
/// it neither broadens the Single/LaterUse product nor limits ordinary program execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowInterleavedAuxiliaryRegion {
    pub(super) interval: UnitBorrowLaterAuxiliaryBindingRegion,
    pub(super) binding_use_position: usize,
    pub(super) uses: Vec<UnitLoanInterleavedAuxiliaryUse>,
}
impl UnitBorrowInterleavedAuxiliaryRegion {
    /// Number of auxiliary returns before the sole binding consumer.
    pub fn binding_use_position(&self) -> usize {
        self.binding_use_position
    }
    /// Actual ordered acquisitions/returns, not a copy of the declaration plan.
    pub fn uses(&self) -> &[UnitLoanInterleavedAuxiliaryUse] {
        &self.uses
    }
    /// Containing callable.
    pub fn callable(&self) -> UnitItemId {
        self.interval.callable()
    }
    /// The sole result binding.
    pub fn binding(&self) -> UnitSymbolId {
        self.interval.binding()
    }
    /// Original source root.
    pub fn root(&self) -> UnitSymbolId {
        self.interval.root()
    }
    /// Original source call.
    pub fn source_call(&self) -> UnitExpressionId {
        self.interval.source_call()
    }
    /// The sole result-binding consumer.
    pub fn consumer_call(&self) -> UnitExpressionId {
        self.interval.consumer_call()
    }
    /// Actual region entry statement.
    pub fn entry_statement(&self) -> UnitStatementId {
        self.interval.entry_statement()
    }
    /// Normal cleanup completion statement.
    pub fn completion_statement(&self) -> UnitStatementId {
        self.interval.completion_statement()
    }
    /// Independent sequence validation result.
    pub fn coverage(&self) -> UnitLoanActionCoverage {
        self.interval.coverage()
    }
    /// Distinct auxiliary root.
    pub fn auxiliary_root(&self) -> Option<UnitSymbolId> {
        self.interval.auxiliary_root()
    }
    /// Positional source argument zero or one.
    pub fn source_parameter(&self) -> Option<usize> {
        self.interval.source_parameter()
    }
    /// Actual source owner required on entry.
    pub fn source_owner(&self) -> Option<&UnitOwnerId> {
        self.interval.source_owner()
    }
    /// Actual auxiliary owner required on entry and across every use.
    pub fn auxiliary_owner(&self) -> Option<&UnitOwnerId> {
        self.interval.auxiliary_owner()
    }
    /// Entire original contiguous action interval, without filtering.
    pub fn action_ids(&self) -> impl ExactSizeIterator<Item = UnitLoanActionId> + '_ {
        self.interval.action_ids()
    }
}
