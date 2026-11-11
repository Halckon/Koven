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

/// Independent two-owner auxiliary retirement at a real normal call return.
/// This is a partial relation, not a complete owner/drop region or executable receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanDistinctAuxiliaryRetirement {
    pub(super) identity: Arc<()>,
    pub(super) source_begin: usize,
    pub(super) auxiliary_begin: usize,
    pub(super) handoff: usize,
    pub(super) auxiliary_drop: Option<usize>,
    pub(super) source_owner: super::UnitOwnerId,
    pub(super) auxiliary_owner: super::UnitOwnerId,
    pub(super) source_parameter: usize,
    pub(super) call: crate::type_checking::UnitExpressionId,
}
impl UnitLoanDistinctAuxiliaryRetirement {
    /// Actual acquisition retained through the handoff.
    pub fn source_begin(&self) -> UnitLoanActionId {
        self.id(self.source_begin)
    }
    /// Actual auxiliary acquisition retired at this normal return.
    pub fn auxiliary_begin(&self) -> UnitLoanActionId {
        self.id(self.auxiliary_begin)
    }
    /// Committed handoff containing the real auxiliary retirement.
    pub fn handoff(&self) -> UnitLoanActionId {
        self.id(self.handoff)
    }
    /// Optional actual auxiliary ASAP Drop between retirement and handoff.
    /// Absence is not a proof that the owner is never dropped later.
    pub fn auxiliary_drop(&self) -> Option<UnitLoanActionId> {
        self.auxiliary_drop.map(|index| self.id(index))
    }
    /// Actual source owner live at retirement; not a future Drop certificate.
    pub fn source_owner(&self) -> &super::UnitOwnerId {
        &self.source_owner
    }
    /// Actual distinct auxiliary owner live at retirement.
    pub fn auxiliary_owner(&self) -> &super::UnitOwnerId {
        &self.auxiliary_owner
    }
    /// Typed source parameter index, limited to zero or one.
    pub fn source_parameter(&self) -> usize {
        self.source_parameter
    }
    /// Normal return where the exact auxiliary pending token was removed.
    pub fn point(&self) -> super::UnitDropPoint {
        super::UnitDropPoint::CallReturn(self.call)
    }
    fn id(&self, index: usize) -> UnitLoanActionId {
        UnitLoanActionId {
            identity: Arc::clone(&self.identity),
            index,
        }
    }
}
