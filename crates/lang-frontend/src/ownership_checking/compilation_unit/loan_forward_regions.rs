//! Independently scoped expression-wrapper normal live-exit certificates.
use super::{UnitLoanActionCoverage, UnitLoanActionId};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::{ops::Range, sync::Arc};

/// One entered expression body and its normal live-source delivery.
/// Complete certifies this declared region, not a source/callable or callee body.
/// A live handoff is not End, permission restoration, or a backend receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowForwardRegion {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitItemId,
    pub(super) parameter: UnitSymbolId,
    pub(super) body: UnitExpressionId,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) records: Range<usize>,
    pub(super) live_handoff: Option<usize>,
}

impl UnitBorrowForwardRegion {
    /// The wrapper owning this normal-exit region, never the inner callee or outer caller.
    #[must_use]
    pub const fn callable(&self) -> UnitItemId {
        self.callable
    }

    /// The sole non-owning Shared String source parameter established at entry.
    #[must_use]
    pub const fn source_parameter(&self) -> UnitSymbolId {
        self.parameter
    }

    /// The exact expression body, also the normal ControlTransfer exit expression.
    #[must_use]
    pub const fn body(&self) -> UnitExpressionId {
        self.body
    }

    /// Independent coverage of this interval. Absence or Partial grants no absence proof.
    #[must_use]
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }

    /// The actual live exit, available only when the entire region is certified.
    #[must_use]
    pub fn live_handoff(&self) -> Option<UnitLoanActionId> {
        self.live_handoff.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }

    /// Every record in the contiguous original interval, without role filtering.
    pub fn action_ids(&self) -> impl ExactSizeIterator<Item = UnitLoanActionId> + '_ {
        self.records.clone().map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
}
