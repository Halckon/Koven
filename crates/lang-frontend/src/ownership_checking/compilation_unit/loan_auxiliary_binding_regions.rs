//! Independent auxiliary-bearing binding coverage; never the legacy seven-record protocol.
use super::{UnitLoanActionCoverage, UnitLoanActionId};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
use std::{ops::Range, sync::Arc};

/// An auxiliary-bearing local binding region entered after its owner has formed.
/// The declared interval starts before the borrow declaration and ends after normal
/// body cleanup. Complete certifies only this interval, not owner formation, the
/// source, the whole callable, or the existing direct-call region protocols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowAuxiliaryBindingRegion {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) binding: UnitSymbolId,
    pub(super) root: UnitSymbolId,
    pub(super) source_call: UnitExpressionId,
    pub(super) consumer_call: UnitExpressionId,
    pub(super) entry_statement: UnitStatementId,
    pub(super) completion_statement: UnitStatementId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) records: Range<usize>,
}

impl UnitBorrowAuxiliaryBindingRegion {
    /// Ordinary caller containing the independently declared statement interval.
    #[must_use]
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }

    /// The initialized non-owning binding, not its canonical source-owner root.
    #[must_use]
    pub const fn binding(&self) -> UnitSymbolId {
        self.binding
    }

    /// The independently established local String owner required at entry.
    #[must_use]
    pub const fn root(&self) -> UnitSymbolId {
        self.root
    }

    /// Direct initializer call handing its source loan to the binding.
    #[must_use]
    pub const fn source_call(&self) -> UnitExpressionId {
        self.source_call
    }

    /// The sole ordinary call using the raw binding operand in this region.
    #[must_use]
    pub const fn consumer_call(&self) -> UnitExpressionId {
        self.consumer_call
    }

    /// Borrow declaration entered after the owner has actually formed.
    #[must_use]
    pub const fn entry_statement(&self) -> UnitStatementId {
        self.entry_statement
    }

    /// Enclosing body whose complete normal cleanup closes the declared interval.
    #[must_use]
    pub const fn completion_statement(&self) -> UnitStatementId {
        self.completion_statement
    }

    /// Independent validation result; missing or partial evidence grants no absence.
    #[must_use]
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }

    /// All records in the original contiguous interval, without action filtering.
    pub fn action_ids(&self) -> impl ExactSizeIterator<Item = UnitLoanActionId> + '_ {
        self.records.clone().map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
}
