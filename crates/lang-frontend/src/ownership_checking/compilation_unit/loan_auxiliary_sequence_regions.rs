//! A separately qualified bounded sequence, never Single/LaterUse coverage.
use super::{
    UnitBorrowLaterAuxiliaryBindingRegion, UnitLoanActionCoverage, UnitLoanActionId, UnitLoanFact,
    UnitOwnerId,
};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
/// One independently observed acquisition and normal return using the auxiliary owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanAuxiliarySequenceUse {
    pub(super) loan: UnitLoanFact,
    pub(super) begin: UnitLoanActionId,
    pub(super) end: UnitLoanActionId,
    pub(super) owner: UnitOwnerId,
}
impl UnitLoanAuxiliarySequenceUse {
    /// Actual pending loan consumed at normal call return.
    pub fn loan(&self) -> &UnitLoanFact {
        &self.loan
    }
    /// Same-catalog actual acquisition record.
    pub fn begin(&self) -> UnitLoanActionId {
        self.begin.clone()
    }
    /// Same-catalog actual normal-return cleanup record.
    pub fn end(&self) -> UnitLoanActionId {
        self.end.clone()
    }
    /// Actual live owner snapshot identity at this return.
    pub fn owner(&self) -> &UnitOwnerId {
        &self.owner
    }
}
/// An independently qualified BA^n interval with a bounded dynamic auxiliary-use sequence.
/// It begins after two owner constructions. Complete covers only this declared interval;
/// it neither broadens the Single/LaterUse product nor limits ordinary program execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowAuxiliarySequenceRegion {
    pub(super) interval: UnitBorrowLaterAuxiliaryBindingRegion,
    pub(super) uses: Vec<UnitLoanAuxiliarySequenceUse>,
}
impl UnitBorrowAuxiliarySequenceRegion {
    /// Actual ordered acquisitions/returns, not a copy of the declaration plan.
    pub fn uses(&self) -> &[UnitLoanAuxiliarySequenceUse] {
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
