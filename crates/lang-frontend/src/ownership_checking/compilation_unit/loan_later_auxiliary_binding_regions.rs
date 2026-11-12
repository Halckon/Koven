//! Independent two-root binding interval with a later auxiliary use.
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
pub struct UnitBorrowLaterAuxiliaryBindingRegion {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) binding: UnitSymbolId,
    pub(super) root: UnitSymbolId,
    pub(super) source_call: UnitExpressionId,
    pub(super) consumer_call: UnitExpressionId,
    pub(super) entry_statement: UnitStatementId,
    pub(super) completion_statement: UnitStatementId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) auxiliary_root: Option<UnitSymbolId>,
    pub(super) source_parameter: Option<usize>,
    pub(super) source_owner: Option<super::UnitOwnerId>,
    pub(super) auxiliary_owner: Option<super::UnitOwnerId>,
    pub(super) later_consumer_call: Option<UnitExpressionId>,
    pub(super) later_statement: Option<UnitStatementId>,
    pub(super) later_begin: Option<UnitLoanActionId>,
    pub(super) later_return_cursor: Option<usize>,
    pub(super) records: Range<usize>,
}

impl UnitBorrowLaterAuxiliaryBindingRegion {
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

    /// The distinct auxiliary root, when declaration admission succeeded.
    #[must_use]
    pub const fn auxiliary_root(&self) -> Option<UnitSymbolId> {
        self.auxiliary_root
    }

    /// The verified positional source parameter, strictly zero or one.
    #[must_use]
    pub const fn source_parameter(&self) -> Option<usize> {
        self.source_parameter
    }

    /// Actual source owner identity observed on interval entry.
    #[must_use]
    pub fn source_owner(&self) -> Option<&super::UnitOwnerId> {
        self.source_owner.as_ref()
    }

    /// Actual distinct auxiliary owner identity observed on interval entry.
    #[must_use]
    pub fn auxiliary_owner(&self) -> Option<&super::UnitOwnerId> {
        self.auxiliary_owner.as_ref()
    }

    /// The independently admitted later auxiliary call.
    #[must_use]
    pub const fn later_consumer_call(&self) -> Option<UnitExpressionId> {
        self.later_consumer_call
    }
    /// Statement containing the later auxiliary use.
    #[must_use]
    pub const fn later_statement(&self) -> Option<UnitStatementId> {
        self.later_statement
    }
    /// Actual acquisition consumed by the observed later normal return.
    #[must_use]
    pub fn later_begin(&self) -> Option<UnitLoanActionId> {
        self.later_begin.clone()
    }
    /// Raw action boundary at the actual later return, not a synthetic End action.
    #[must_use]
    pub const fn later_return_cursor(&self) -> Option<usize> {
        self.later_return_cursor
    }

    /// All records in the original contiguous interval, without action filtering.
    pub fn action_ids(&self) -> impl ExactSizeIterator<Item = UnitLoanActionId> + '_ {
        self.records.clone().map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
}
