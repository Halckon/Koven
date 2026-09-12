//! Phase 3 变量所有权状态与 use-after-move 检查。

mod capture;
mod checker;
mod compilation_unit;
mod constant;
mod construction;
mod error;
mod model;
mod non_null_assertion;
mod nullable_when;
mod rc;

use crate::{
    name_resolution::NameResolution, parser::ParsedFile, source::SourceMap,
    type_checking::TypedFile,
};

pub use compilation_unit::{
    CompilationUnitConstantOwnership, CompilationUnitOwnership, ConstEnabledOwnedUnit,
    UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind, UnitClosureCaptureDescriptor,
    UnitClosureCaptureSource, UnitClosureDescriptor, UnitConditionalReceiverDeliveryFact,
    UnitConditionalReceiverDropFact, UnitConstantMaterializationPlan,
    UnitConstructionDeliveryEffect, UnitConstructionOwnershipPlan,
    UnitConstructionRootDropObligation, UnitDelegationOwnershipPlan, UnitDropFact, UnitDropPoint,
    UnitDropTarget, UnitLoanFact, UnitLoanTarget, UnitNonNullAssertionOwnershipPlan,
    UnitOwnershipBindingDescriptor, UnitOwnershipDeferredFact, UnitOwnershipPlace,
    UnitRcOwnershipEffect, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
    UnitReceiverOwnershipTarget, UnitValueDeliveryFact, UnitValueDeliveryKind,
    UnitValueDeliverySource, ValidatedCompilationUnitOwnership,
    check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
};
pub use constant::{
    ConstantMaterializationKind, ConstantMaterializationPlan, ValidatedConstantMaterializations,
};
pub use construction::{
    ConstructionDeliveryEffect, ConstructionDeliveryKind, ConstructionOwnershipPlan,
    ConstructionRootDropObligation, ConstructionRootKind,
};
pub use error::OwnershipCheckingError;
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
