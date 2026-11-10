//! Actual planner actions with separately scoped normal-edge region certificates.
use super::{UnitDropPoint, UnitIterationCleanupAction, UnitLoanFact};
use crate::name_resolution::SourceUnitId;
use std::sync::Arc;

/// Coverage of a declared source or normal-edge region, never implicit callable completeness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitLoanActionCoverage {
    /// A separately validated declared region; never a whole-source certificate.
    Complete,
    /// Actual actions are present, but unregistered lifecycle families remain.
    Partial,
    /// No supported action was observed. Empty does not prove absence.
    Unsupported,
}

/// A record appended at the actual acquisition or cleanup publication site.
/// Record order is planner traversal order, not a cross-path execution timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitLoanAction {
    /// The evaluated argument registered this exact pending call token.
    Begin(UnitLoanFact),
    /// The evaluated Shared receiver registered its actual pending token.
    ReceiverBegin(super::UnitReceiverOwnershipFact),
    /// Normal-return retirement, observed without adding an execution cleanup.
    ReceiverEnd(super::UnitReceiverCallReturnEndFact),
    /// Successful initializer commit of the existing source-to-binding handoff.
    /// Its historical CallReturn/auxiliary descriptors are not new End events here.
    BorrowHandoffCommitted(super::UnitBorrowCallHandoffFact),
    /// Actual removal of an initialized borrow binding, before its owner cleanup.
    /// This record is not a generic parent-permission restoration certificate.
    BorrowBindingEnd(
        crate::ownership_checking::BorrowBindingEndFact<
            crate::name_resolution::UnitSymbolId,
            UnitDropPoint,
        >,
    ),
    /// An action from the single existing cleanup source, without sorting old tables.
    Cleanup {
        /// Source-qualified cleanup point; separate records are never coalesced by span.
        point: UnitDropPoint,
        /// Existing planner action. This observation does not execute it again.
        action: UnitIterationCleanupAction,
    },
}

/// Opaque identity of one static record in one observation catalog.
/// This is not a dynamic loan/activation identity.
#[derive(Clone, Debug)]
pub struct UnitLoanActionId {
    pub(super) identity: Arc<()>,
    pub(super) index: usize,
}

impl PartialEq for UnitLoanActionId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity) && self.index == other.index
    }
}
impl Eq for UnitLoanActionId {}

/// Source-scoped partial evidence, not an absence or whole-callable oracle.
/// Equality compares structural snapshots, not catalog identity. Action IDs are catalog-bound.
/// Callable/path segments and dynamic activations are not certified. Equal descriptors or
/// points cannot identify a dynamic loan or compare execution across branches/loops/callables.
/// Result handoff, receiver activation and permission restoration remain uncovered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanActionObservation {
    pub(super) identity: Arc<()>,
    pub(super) source: SourceUnitId,
    pub(super) actions: Vec<UnitLoanAction>,
    pub(super) regions: Vec<super::UnitDirectCallLoanRegion>,
}

impl UnitLoanActionObservation {
    /// Entered ordinary-call regions. Missing entries do not prove absence.
    #[must_use]
    pub fn regions(&self) -> &[super::UnitDirectCallLoanRegion] {
        &self.regions
    }

    /// Check the exact catalog of a region, including a Complete-empty certificate.
    /// Cloning preserves membership; equal source/call indices from reanalysis do not.
    #[must_use]
    pub fn owns_region(&self, region: &super::UnitDirectCallLoanRegion) -> bool {
        Arc::ptr_eq(&self.identity, &region.identity)
    }

    /// Identify a static record without accepting identities from another observation.
    #[must_use]
    pub fn action_id(&self, index: usize) -> Option<UnitLoanActionId> {
        self.actions.get(index).map(|_| UnitLoanActionId {
            identity: Arc::clone(&self.identity),
            index,
        })
    }

    /// Resolve only a record from this catalog; reject catalogs created by reanalysis.
    #[must_use]
    pub fn action(&self, id: &UnitLoanActionId) -> Option<&UnitLoanAction> {
        if !Arc::ptr_eq(&self.identity, &id.identity) {
            return None;
        }
        self.actions.get(id.index)
    }

    /// Same-analysis source identity of every record.
    #[must_use]
    pub const fn source(&self) -> SourceUnitId {
        self.source
    }

    /// Actual source records, including repeated distinct same-point actions.
    #[must_use]
    pub fn actions(&self) -> &[UnitLoanAction] {
        &self.actions
    }

    /// This incremental producer cannot certify Complete, including Complete-empty.
    #[must_use]
    pub fn coverage(&self) -> UnitLoanActionCoverage {
        if self.actions.is_empty() {
            UnitLoanActionCoverage::Unsupported
        } else {
            UnitLoanActionCoverage::Partial
        }
    }
}

impl super::CompilationUnitOwnership {
    /// Actual acquisition/cleanup source observation. All entries remain Partial/Unsupported.
    /// Errors and deferred analysis revoke both raw and public observations for the whole unit.
    #[must_use]
    pub fn loan_action_observations(&self) -> &[UnitLoanActionObservation] {
        &self.loan_action_observations
    }
}
