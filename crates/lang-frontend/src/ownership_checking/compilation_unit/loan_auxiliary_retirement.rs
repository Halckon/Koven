//! Catalog-bound partial auxiliary retirement evidence, independent of region coverage.
use super::UnitLoanActionId;
use std::sync::Arc;

/// A relation from original acquisitions to a successful borrowed-result handoff.
/// This is not an End event, complete lifecycle region, or executable receipt.
/// Equality compares structural snapshots; action IDs retain catalog identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanAuxiliaryRetirement {
    pub(super) identity: Arc<()>,
    pub(super) source_begin: usize,
    pub(super) auxiliary_begin: usize,
    pub(super) handoff: usize,
}

impl UnitLoanAuxiliaryRetirement {
    /// Actual acquisition of the source retained through this handoff.
    #[must_use]
    pub fn source_begin(&self) -> UnitLoanActionId {
        self.action_id(self.source_begin)
    }

    /// Actual acquisition of the distinct auxiliary token retired at normal return.
    #[must_use]
    pub fn auxiliary_begin(&self) -> UnitLoanActionId {
        self.action_id(self.auxiliary_begin)
    }

    /// Original committed handoff containing the auxiliary retirement fact.
    #[must_use]
    pub fn handoff(&self) -> UnitLoanActionId {
        self.action_id(self.handoff)
    }

    fn action_id(&self, index: usize) -> UnitLoanActionId {
        UnitLoanActionId {
            identity: Arc::clone(&self.identity),
            index,
        }
    }
}
