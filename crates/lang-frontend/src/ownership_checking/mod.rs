//! Phase 3 变量所有权状态与 use-after-move 检查。

mod borrow_last_use;
mod borrow_result;
mod callable_provenance;
mod capture;
mod capture_rules;
mod checked_file;
mod checker;
mod cleanup_condition;
mod compilation_unit;
mod const_handoff;
mod constant;
mod construction;
mod element_index;
mod error;
mod handoff;
mod iteration;
mod iteration_source;
mod loop_no_carry;
mod map_iteration;
mod map_iteration_validation;
pub use iteration_source::IterationSource;
pub use map_iteration::{
    MapIterationComponent, MapIterationLoan, MapIterationOwnershipPlan, MapIterationSourceAccess,
};
mod model;
mod non_null_assertion;
mod nullable_when;
mod ownership_primitive;
mod range_producer;
pub(crate) use compilation_unit::check_compilation_unit_ownership_recovering;
mod rc;
mod string;

use crate::{
    name_resolution::NameResolution, parser::ParsedFile, source::SourceMap,
    type_checking::TypedFile,
};

pub use borrow_result::{
    BorrowBindingEndFact, BorrowBindingFact, BorrowBindingStorage, BorrowCallUseFact,
    BorrowResultFacts, BorrowReturnOriginFact, BorrowSourceLoan, RangeReturnOriginFact,
    RangeUseFact, RangeUseSite,
};
pub use callable_provenance::{
    CallableOrigin, CallableOriginFact, PointerCallableReturnOrigin, PointerCallableReturnSummary,
    UnitCallableOrigin, UnitCallableOriginFact, UnitClosureCallableReturnSummary,
    UnitPointerCallableReturnOrigin, UnitPointerCallableReturnSummary,
};
pub use cleanup_condition::{
    CleanupCaptureEdge, CleanupCaptureInput, CleanupCaptureSlot, CleanupCaptureSlotId,
    CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
    CleanupInstanceAddress, CleanupInstanceAddressId, CleanupOwnerInput, CleanupOwnerSnapshot,
    CleanupOwnerValue, CleanupOwnerValueId, CleanupSelection, CleanupSelector, CleanupSelectorCopy,
    CleanupSelectorId, CleanupSelectorSource,
};
pub use compilation_unit::{
    CompilationUnitConstantOwnership, CompilationUnitOwnership, ConstEnabledOwnedUnit,
    UnitAuxiliarySourceState, UnitBindingUseReturn, UnitBorrowAuxiliaryBindingRegion,
    UnitBorrowAuxiliarySequenceRegion, UnitBorrowBindingRegion, UnitBorrowBindingTransportFact,
    UnitBorrowBranchForwardPath, UnitBorrowBranchForwardRegion, UnitBorrowBranchForwardTopology,
    UnitBorrowCallHandoffFact, UnitBorrowCallPrefixEndFact, UnitBorrowCallTransportFact,
    UnitBorrowChildRegion, UnitBorrowDistinctAuxiliaryBindingRegion, UnitBorrowForwardRegion,
    UnitBorrowForwardTransportFact, UnitBorrowInterleavedAuxiliaryRegion,
    UnitBorrowLaterAuxiliaryBindingRegion, UnitBorrowMultiBindingRegion,
    UnitBorrowNestedForwardCascadeContinuation, UnitBorrowNestedForwardContinuation,
    UnitBorrowNestedForwardFlagLayout, UnitBorrowNestedForwardRegion,
    UnitBorrowNestedForwardTopology, UnitBoundedBorrowContinuation,
    UnitBoundedBorrowContinuationRegion, UnitBoundedBorrowForwardPath,
    UnitBoundedBorrowForwardRegion, UnitBoundedBorrowMultiContinuationRegion,
    UnitBoundedBorrowMultiJoin, UnitBoundedBorrowSharedTail, UnitBoundedBorrowTailOrigin,
    UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind, UnitClosureCaptureDescriptor,
    UnitClosureCaptureSource, UnitClosureDescriptor, UnitConditionalReceiverDeliveryFact,
    UnitConditionalReceiverDropFact, UnitConstantMaterializationPlan,
    UnitConstructionDeliveryEffect, UnitConstructionOwnershipPlan,
    UnitConstructionRootDropObligation, UnitDelegationOwnershipPlan, UnitDirectCallLoanEndFact,
    UnitDirectCallLoanRegion, UnitDropFact, UnitDropPoint, UnitDropTarget,
    UnitExclusiveCallPrefixEndFact, UnitIterationCleanupAction, UnitIterationExitKind,
    UnitIterationExitPlan, UnitIterationOwnershipPlan, UnitIterationSourceAccess, UnitLoanAction,
    UnitLoanActionCoverage, UnitLoanActionId, UnitLoanActionObservation,
    UnitLoanAuxiliaryRetirement, UnitLoanAuxiliarySequenceUse, UnitLoanBindingChildCommit,
    UnitLoanBindingChildRetirement, UnitLoanBindingChildUseReturn, UnitLoanBindingDependencyDrop,
    UnitLoanBindingSourceRetirement, UnitLoanBindingUseReturn, UnitLoanBindingUseSet,
    UnitLoanBindingUseSetRetirement, UnitLoanConsumerRetirement,
    UnitLoanDistinctAuxiliaryRetirement, UnitLoanFact, UnitLoanForwardBranch,
    UnitLoanForwardHandoff, UnitLoanInterleavedAuxiliaryUse, UnitLoanLoopInvariantLineage,
    UnitLoanNestedForwardEdge, UnitLoanNestedForwardLineage, UnitLoanRegionFamily, UnitLoanTarget,
    UnitLoopNoCarryFact, UnitMapIterationLoan, UnitMapIterationOwnershipPlan,
    UnitMultiBindingAuxiliaryUse, UnitMultiBindingSourceState, UnitNonNullAssertionOwnershipPlan,
    UnitNullableWhenEntryOwnershipFact, UnitNullableWhenOwnershipPlan, UnitNullableWhenUseFact,
    UnitNullableWhenUseKind, UnitOwnerBridgeCounterEntryV1, UnitOwnerBridgeCounterTailV1,
    UnitOwnerCallable, UnitOwnerChoiceIdV1, UnitOwnerChoiceInputV1, UnitOwnerChoiceV1,
    UnitOwnerClosedLoopBatchV1, UnitOwnerConditionalBreakBackedgeV1,
    UnitOwnerConditionalReturnBackedgeV1, UnitOwnerContinueArmBoundaryV1,
    UnitOwnerContinueBackedgeV1, UnitOwnerControlBridgeEvidenceV1, UnitOwnerControlChainBodiesV1,
    UnitOwnerControlChainBodyV1, UnitOwnerControlChainLeadingV1, UnitOwnerCounterBatchEvidenceV1,
    UnitOwnerCounterTailEvidenceV1, UnitOwnerDefinition, UnitOwnerHeaderSourceV1, UnitOwnerId,
    UnitOwnerInvariantBreakObservationV1, UnitOwnerInvariantLoopObservationV1,
    UnitOwnerInvariantMixedJoinObservationV1, UnitOwnerInvariantNormalJoinObservationV1,
    UnitOwnerInvariantSplitContinueObservationV1, UnitOwnerInvariantSplitExitObservationV1,
    UnitOwnerJumpArmBoundaryV1, UnitOwnerLoopBindingV1, UnitOwnerLoopEdgeV1,
    UnitOwnerLoopObservationV1, UnitOwnerMixedArmBoundaryV1, UnitOwnerMixedBackedgeV1,
    UnitOwnerMultiRootBoundaryV1, UnitOwnerMultiRootNormalTransportV1, UnitOwnerNestedNormalBodyV1,
    UnitOwnerNestedParentBoundaryV1, UnitOwnerNestedResumeWriteV1, UnitOwnerNormalArmBoundaryV1,
    UnitOwnerNormalBackedgeV1, UnitOwnerNormalBindDropV1, UnitOwnerObservation,
    UnitOwnerObservationStatus, UnitOwnerOpenLoopObservationV1, UnitOwnerOpenLoopPrefixV1,
    UnitOwnerOpenLoopReturnV1, UnitOwnerOrigin, UnitOwnerProducerCatalogV1,
    UnitOwnerReplaceBreakCallActionV1, UnitOwnerReplaceBreakCallConsumerV1,
    UnitOwnerReplaceBreakConsumerActionV1, UnitOwnerReplaceBreakConsumerV1,
    UnitOwnerReplaceBreakObservationV1, UnitOwnerReplaceBreakReturnActionV1,
    UnitOwnerReplaceBreakReturnConsumerV1, UnitOwnerReplaceBreakStateV1, UnitOwnerReturnActionV1,
    UnitOwnerReturnEndpointV1, UnitOwnerRootBoundarySlotV1, UnitOwnerRootEdgeV1,
    UnitOwnerRootExitPartitionV1, UnitOwnerRootReplaceStepV1, UnitOwnerTransportEvent,
    UnitOwnerUnsupportedReason, UnitOwnerValueEventV1, UnitOwnerValueObservationV1,
    UnitOwnerValueV1, UnitOwnershipBindingDescriptor, UnitOwnershipDeferredFact,
    UnitOwnershipPlace, UnitRcOwnershipEffect, UnitReceiverCallPrefixEndFact,
    UnitReceiverCallReturnEndFact, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
    UnitReceiverOwnershipTarget, UnitShortCircuitPlan, UnitShortCircuitRhs, UnitValueDeliveryFact,
    UnitValueDeliveryKind, UnitValueDeliverySource, ValidatedCompilationUnitOwnership,
    check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
};
pub use const_handoff::{ConstOwnedCompilationUnitView, const_owned_compilation_unit_view};
pub use constant::{
    ConstantMaterializationKind, ConstantMaterializationPlan, ValidatedConstantMaterializations,
};
pub use construction::{
    ConstructionDeliveryEffect, ConstructionDeliveryKind, ConstructionOwnershipPlan,
    ConstructionRootDropObligation, ConstructionRootKind,
};
pub use error::OwnershipCheckingError;
mod iteration_validation;
pub use handoff::{
    OwnedCompilationUnitView, OwnedCompilationUnitViewError, owned_compilation_unit_view,
};
pub use iteration::{
    ClosureReleaseLayout, IterationCaptureGraph, IterationCaptureNode, IterationCaptureSource,
    IterationCleanupAction, IterationClosureBinding, IterationClosureFlow,
    IterationClosurePhiBinding, IterationClosurePhiOrigin, IterationClosurePhiSource,
    IterationExitKind, IterationExitPlan, IterationOwnershipPlan, IterationPhiBoundary,
    IterationPhiCaptureSlot, IterationPhiIncoming, IterationPhiIncomingBinding,
    IterationPhiIncomingEnvironment, IterationPhiIncomingKind, IterationPhiIncomingOrigin,
    IterationPhiIncomingSource, IterationPhiIncomingValue, IterationPhiPresenceSource,
    IterationPhiRootSource, IterationPhiSelectorWrite,
};
pub use iteration_validation::{IterationFactError, validate_iteration_facts};
pub use model::{
    ClosureCaptureDescriptor, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
    ClosureDescriptor, DropFact, DropPoint, DropTarget, ElementIndexIdentity, LoanEndFact,
    LoanEndPoint, LoanFact, LoanKind, LoanTarget, OwnershipBindingDescriptor, OwnershipBindingKind,
    OwnershipCheckedFile, OwnershipDeferredFact, OwnershipDeferredReason, OwnershipPlace,
    Transferability,
};
pub use non_null_assertion::{NonNullAssertionOwnershipPlan, NonNullAssertionTransferKind};
pub use nullable_when::{
    NullableWhenBranchFact, NullableWhenBranchOutcome, NullableWhenExtractionFact,
    NullableWhenExtractionKind, NullableWhenOwnershipPlan, NullableWhenProofView,
};
pub use rc::{RcOwnershipEffect, RcOwnershipEffectKind};
pub use string::{StringOwnershipEffect, UnitStringCloneLoanEndFact, UnitStringOwnershipEffect};

/// 对同一源码的名称、类型产物执行变量所有权检查。
pub fn check_ownership(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<OwnershipCheckedFile, OwnershipCheckingError> {
    let source_id = parsed.source_id();
    if names.source_id() != source_id {
        return Err(OwnershipCheckingError::MismatchedNameSource);
    }
    if typed.source_id() != source_id {
        return Err(OwnershipCheckingError::MismatchedTypedSource);
    }
    if !typed.is_compatible_with_names(names) {
        return Err(OwnershipCheckingError::MismatchedAnalysisIdentity);
    }
    checker::check(sources, parsed, names, typed)
}

pub use ownership_primitive::{
    FieldReplaceOwnershipPlan, OwnershipPrimitiveOwnershipPlan, OwnershipPrimitiveValueTransfer,
    UnitFieldReplaceOwnershipPlan, UnitOwnershipPrimitiveOwnershipPlan,
};

#[cfg(feature = "loan-observation-test-support")]
#[doc(hidden)]
pub use compilation_unit::loan_observation_test_support::{
    legacy_ownership_eq, with_loan_observation,
};

#[cfg(any(test, feature = "loan-observation-test-support"))]
mod research_borrow_frame;
#[cfg(any(test, feature = "loan-observation-test-support"))]
mod research_borrow_zero_loop;
#[cfg(any(test, feature = "loan-observation-test-support"))]
#[doc(hidden)]
pub use research_borrow_frame::{
    BorrowFrameChecker, BorrowFrameConsumerReturn, BorrowFrameObservation, capture_borrow_frames,
};
#[cfg(any(test, feature = "loan-observation-test-support"))]
pub use research_borrow_zero_loop::{
    BorrowZeroBodyFrame, BorrowZeroLoopObservation, capture_borrow_zero_loops,
};

#[cfg(any(test, feature = "loan-observation-test-support"))]
mod research_borrow_loop;
#[cfg(any(test, feature = "loan-observation-test-support"))]
#[doc(hidden)]
pub use research_borrow_loop::{
    BorrowLoopBinding, BorrowLoopConsumerReturn, BorrowLoopEdgeKind, BorrowLoopEdgeObservation,
    BorrowLoopExpression, capture_borrow_loop_edges,
};

#[cfg(any(test, feature = "loan-observation-test-support"))]
mod research_borrow_consumer_body;
#[cfg(any(test, feature = "loan-observation-test-support"))]
pub use research_borrow_consumer_body::{
    BorrowBodyConsumerReturn, BorrowConsumerBodyObservation, capture_borrow_consumer_bodies,
};
