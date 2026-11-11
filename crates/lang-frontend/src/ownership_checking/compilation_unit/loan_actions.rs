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
    /// Actual return of a call using a live local borrow binding; not a binding end.
    BorrowBindingUseReturned(super::UnitLoanBindingUseReturn),
    /// Actual retirement of the source carried by a removed local borrow binding.
    BorrowBindingSourceRetired(super::UnitLoanBindingSourceRetirement),
    /// Actual initialization of a direct child of a live result binding.
    BorrowBindingChildCommitted(super::UnitLoanBindingChildCommit),
    /// Actual return of a call using that child; not a child or parent binding end.
    BorrowBindingChildUseReturned(super::UnitLoanBindingChildUseReturn),
    /// Actual child removal while the original parent source transport remains live.
    BorrowBindingChildRetired(super::UnitLoanBindingChildRetirement),
    /// Actual move of a source token into synchronous consumer transport; not an End.
    BorrowConsumerTransfer {
        /// The exact moved source token, not a reconstructed parent.
        source: UnitLoanFact,
        /// The selected synchronous consumer.
        consumer: crate::type_checking::UnitExpressionId,
        /// Its argument receiving the borrowed result, including transparent Group syntax.
        argument: crate::type_checking::UnitExpressionId,
    },
    /// Actual consumer/source retirement with a separately verified transfer reference.
    BorrowConsumerRetired(UnitLoanConsumerRetirement),
    /// Actual wrapper delivery of a live source, with an optional validated acquisition link.
    /// This is not a source End or a complete wrapper lifecycle certificate.
    BorrowForwardHandoff(UnitLoanForwardHandoff),
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
/// General result-continuation, receiver-activation and permission-restoration coverage remains unproved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanActionObservation {
    pub(super) identity: Arc<()>,
    pub(super) source: SourceUnitId,
    pub(super) actions: Vec<UnitLoanAction>,
    pub(super) regions: Vec<super::UnitDirectCallLoanRegion>,
    pub(super) binding_regions: Vec<super::UnitBorrowBindingRegion>,
    pub(super) child_regions: Vec<super::UnitBorrowChildRegion>,
    pub(super) forward_regions: Vec<super::UnitBorrowForwardRegion>,
    pub(super) nested_forward_regions: Vec<super::UnitBorrowNestedForwardRegion>,
    pub(super) nested_forward_lineages: Vec<super::UnitLoanNestedForwardLineage>,
    pub(super) branch_forward_regions: Vec<super::UnitBorrowBranchForwardRegion>,
    pub(super) binding_dependency_drops: Vec<super::UnitLoanBindingDependencyDrop>,
    pub(super) auxiliary_retirements: Vec<super::UnitLoanAuxiliaryRetirement>,
    pub(super) auxiliary_binding_regions: Vec<super::UnitBorrowAuxiliaryBindingRegion>,
}

impl UnitLoanActionObservation {
    /// Independent bounded nested closure; absence grants no proof or backend admission.
    pub fn nested_forward_regions(&self) -> &[super::UnitBorrowNestedForwardRegion] {
        &self.nested_forward_regions
    }
    /// Require the exact originating observation catalog.
    pub fn owns_nested_forward_region(
        &self,
        region: &super::UnitBorrowNestedForwardRegion,
    ) -> bool {
        Arc::ptr_eq(&self.identity, &region.identity)
    }

    /// Partial actual nested ancestry; absence grants no proof and enables no receipt.
    pub fn nested_forward_lineages(&self) -> &[super::UnitLoanNestedForwardLineage] {
        &self.nested_forward_lineages
    }
    /// Require the original observation catalog, not an equal snapshot.
    pub fn owns_nested_forward_lineage(
        &self,
        lineage: &super::UnitLoanNestedForwardLineage,
    ) -> bool {
        Arc::ptr_eq(&self.identity, &lineage.identity)
    }

    /// Independent auxiliary-bearing binding intervals, not legacy binding certificates.
    /// Missing entries grant no absence proof or backend receipt authority.
    #[must_use]
    pub fn auxiliary_binding_regions(&self) -> &[super::UnitBorrowAuxiliaryBindingRegion] {
        &self.auxiliary_binding_regions
    }

    /// Check the original catalog, never an equal snapshot from another analysis.
    #[must_use]
    pub fn owns_auxiliary_binding_region(
        &self,
        region: &super::UnitBorrowAuxiliaryBindingRegion,
    ) -> bool {
        Arc::ptr_eq(&self.identity, &region.identity)
    }

    /// Partial relations to original normal-return auxiliary retirements.
    /// Empty grants no absence proof and does not enable any region or receipt.
    #[must_use]
    pub fn auxiliary_retirements(&self) -> &[super::UnitLoanAuxiliaryRetirement] {
        &self.auxiliary_retirements
    }

    /// Independent coverage of the narrowly declared branch region.
    /// Absence grants no absence proof and never enables expression-wrapper receipts.
    #[must_use]
    pub fn branch_forward_regions(&self) -> &[super::UnitBorrowBranchForwardRegion] {
        &self.branch_forward_regions
    }

    /// Check catalog identity, never an equal structural snapshot.
    pub fn owns_branch_forward_region(
        &self,
        region: &super::UnitBorrowBranchForwardRegion,
    ) -> bool {
        Arc::ptr_eq(&self.identity, &region.identity)
    }

    /// Independently entered wrapper normal-exit regions. Missing entries prove no absence.
    #[must_use]
    pub fn forward_regions(&self) -> &[super::UnitBorrowForwardRegion] {
        &self.forward_regions
    }

    /// Check the exact observation catalog, not an equal callable or source descriptor.
    #[must_use]
    pub fn owns_forward_region(&self, region: &super::UnitBorrowForwardRegion) -> bool {
        Arc::ptr_eq(&self.identity, &region.identity)
    }

    /// Independently entered direct parent/child regions. Missing entries prove no absence.
    #[must_use]
    pub fn child_regions(&self) -> &[super::UnitBorrowChildRegion] {
        &self.child_regions
    }

    /// Check the exact observation catalog rather than equal binding descriptors.
    #[must_use]
    pub fn owns_child_region(&self, region: &super::UnitBorrowChildRegion) -> bool {
        Arc::ptr_eq(&self.identity, &region.identity)
    }

    /// Verified local parent/child dependency-to-Drop relations from actual cleanup publication.
    /// Empty does not prove absence; entries do not certify whole-lifecycle coverage.
    #[must_use]
    pub fn binding_dependency_drops(&self) -> &[super::UnitLoanBindingDependencyDrop] {
        &self.binding_dependency_drops
    }

    /// Independently entered binding regions. Missing entries do not prove absence.
    #[must_use]
    pub fn binding_regions(&self) -> &[super::UnitBorrowBindingRegion] {
        &self.binding_regions
    }

    /// Check the exact observation catalog, not equal source or binding descriptors.
    #[must_use]
    pub fn owns_binding_region(&self, region: &super::UnitBorrowBindingRegion) -> bool {
        Arc::ptr_eq(&self.identity, &region.identity)
    }

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

/// Partial evidence of actual synchronous retirement; not a complete lifecycle certificate.
/// Equality compares structural snapshots. The exposed transfer ID remains catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanConsumerRetirement {
    pub(super) identity: Arc<()>,
    pub(super) transfer: Option<usize>,
    pub(super) source_begin: Option<usize>,
    pub(super) consumer_begin: Option<usize>,
    pub(super) transaction: super::UnitBorrowCallTransportFact,
}
impl UnitLoanConsumerRetirement {
    /// Validated actual source acquisition; None is incomplete evidence, not absence.
    #[must_use]
    pub fn source_begin(&self) -> Option<UnitLoanActionId> {
        self.source_begin.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
    /// Validated actual consumer acquisition from the retired pending token.
    #[must_use]
    pub fn consumer_begin(&self) -> Option<UnitLoanActionId> {
        self.consumer_begin.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
    /// Validated reference to the actual earlier transfer record, when identity is proved.
    /// None means the observation did not prove this edge, never absence of an obligation.
    #[must_use]
    pub fn transfer(&self) -> Option<UnitLoanActionId> {
        self.transfer.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
    /// The existing real consumer-then-source retirement transaction.
    #[must_use]
    pub const fn transaction(&self) -> &super::UnitBorrowCallTransportFact {
        &self.transaction
    }
}

/// Partial evidence of live-source delivery at an actual wrapper return.
/// Equality compares structural snapshots; the acquisition ID remains catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanForwardHandoff {
    pub(super) identity: Arc<()>,
    pub(super) source_begin: Option<usize>,
    pub(super) callable: Option<crate::type_checking::UnitItemId>,
    pub(super) transaction: super::UnitBorrowForwardTransportFact,
    pub(super) branch: Option<UnitLoanForwardBranch>,
}
/// A normal branch edge actually entered by the planner, not whole-region coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitLoanForwardBranch {
    pub(super) callable: crate::type_checking::UnitItemId,
    pub(super) split: crate::type_checking::UnitExpressionId,
    pub(super) condition: crate::type_checking::UnitExpressionId,
    pub(super) then_arm: bool,
    pub(super) control: crate::type_checking::UnitExpressionId,
    pub(super) value: crate::type_checking::UnitExpressionId,
}
impl UnitLoanForwardBranch {
    /// Enclosing wrapper declaration, qualified by its source unit.
    pub const fn callable(&self) -> crate::type_checking::UnitItemId {
        self.callable
    }
    /// The actual If expression whose edge was entered.
    pub const fn split(&self) -> crate::type_checking::UnitExpressionId {
        self.split
    }
    /// The condition evaluated before entering this edge.
    pub const fn condition(&self) -> crate::type_checking::UnitExpressionId {
        self.condition
    }
    /// True for the then edge; false for its sole normal continuation.
    pub const fn then_arm(&self) -> bool {
        self.then_arm
    }
    /// The explicit Return control, distinct from the returned call.
    pub const fn control(&self) -> crate::type_checking::UnitExpressionId {
        self.control
    }
    /// The actual returned call whose source acquisition is delivered.
    pub const fn value(&self) -> crate::type_checking::UnitExpressionId {
        self.value
    }
}
impl UnitLoanForwardHandoff {
    /// Validated actual branch entry. None grants no path or absence proof.
    pub const fn branch(&self) -> Option<UnitLoanForwardBranch> {
        self.branch
    }
    /// Validated actual acquisition carried through the pending source move.
    /// None means this edge is unproved, never that the live obligation is absent.
    #[must_use]
    pub fn source_begin(&self) -> Option<UnitLoanActionId> {
        self.source_begin.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
    /// The enclosing wrapper, not the inner callee or its caller.
    /// None means this bounded callable identity was not proved.
    #[must_use]
    pub const fn callable(&self) -> Option<crate::type_checking::UnitItemId> {
        self.callable
    }
    /// The unchanged real forwarding transaction; its source is still live.
    #[must_use]
    pub const fn transaction(&self) -> &super::UnitBorrowForwardTransportFact {
        &self.transaction
    }
}
