//! SPEC-0198 compilation-unit 所有权产物、身份门禁与 body-local call 数据流。
pub(super) mod loop_no_carry;
pub use loop_no_carry::{UnitLoopBranchNoCarryFact, UnitLoopNoCarryFact};
mod loan_bounded_forward_regions;
pub use loan_bounded_forward_regions::{
    UnitBoundedBorrowContinuation, UnitBoundedBorrowContinuationRegion,
    UnitBoundedBorrowForwardPath, UnitBoundedBorrowForwardRegion,
    UnitBoundedBorrowMultiContinuationRegion, UnitBoundedBorrowMultiJoin,
    UnitBoundedBorrowSharedTail, UnitBoundedBorrowTailOrigin,
};
mod drop_point;
pub use drop_point::{
    UnitConditionalReceiverDropFact, UnitDropFact, UnitDropPoint, UnitDropTarget,
};

mod loan_region_validation;
mod loan_regions;
pub use loan_regions::{UnitDirectCallLoanRegion, UnitLoanRegionFamily};
mod loan_auxiliary_binding_region_validation;
mod loan_auxiliary_binding_regions;
mod loan_binding_region_validation;
mod loan_binding_regions;
pub use loan_auxiliary_binding_regions::UnitBorrowAuxiliaryBindingRegion;
mod loan_branch_forward_regions;
mod loan_nested_forward_lineage;
mod loan_nested_forward_regions;
pub use loan_nested_forward_lineage::{UnitLoanNestedForwardEdge, UnitLoanNestedForwardLineage};
pub use loan_nested_forward_regions::{
    UnitBorrowNestedForwardCascadeContinuation, UnitBorrowNestedForwardContinuation,
    UnitBorrowNestedForwardFlagLayout, UnitBorrowNestedForwardRegion,
    UnitBorrowNestedForwardTopology,
};
mod loan_child_region_validation;
mod loan_child_regions;
mod loan_forward_regions;
pub use loan_binding_regions::UnitBorrowBindingRegion;
pub use loan_branch_forward_regions::{
    UnitBorrowBranchForwardPath, UnitBorrowBranchForwardRegion, UnitBorrowBranchForwardTopology,
};
pub use loan_child_regions::UnitBorrowChildRegion;
pub use loan_forward_regions::UnitBorrowForwardRegion;
mod loan_binding_dependency_drop;
pub use loan_binding_dependency_drop::UnitLoanBindingDependencyDrop;
mod loan_binding_child_actions;
pub use loan_binding_child_actions::{
    UnitLoanBindingChildCommit, UnitLoanBindingChildRetirement, UnitLoanBindingChildUseReturn,
};
mod loan_binding_actions;
pub use loan_binding_actions::{UnitLoanBindingSourceRetirement, UnitLoanBindingUseReturn};
mod loan_actions;
mod loan_auxiliary_retirement;
mod loan_auxiliary_sequence_regions;
mod loan_binding_use_sets;
mod loan_loop_invariant;
pub use loan_loop_invariant::UnitLoanLoopInvariantLineage;
mod loan_multi_binding_regions;
pub use loan_multi_binding_regions::{
    UnitBorrowMultiBindingRegion, UnitMultiBindingAuxiliaryUse, UnitMultiBindingSourceState,
};
mod loan_interleaved_auxiliary_regions;
pub use loan_binding_use_sets::{
    UnitBindingUseReturn, UnitLoanBindingUseSet, UnitLoanBindingUseSetRetirement,
};
pub use loan_interleaved_auxiliary_regions::{
    UnitAuxiliarySourceState, UnitBorrowInterleavedAuxiliaryRegion, UnitLoanInterleavedAuxiliaryUse,
};
mod loan_distinct_auxiliary_binding_region_validation;
mod loan_distinct_auxiliary_binding_regions;
mod loan_later_auxiliary_binding_region_validation;
mod loan_later_auxiliary_binding_regions;
pub use loan_actions::{
    UnitLoanAction, UnitLoanActionCoverage, UnitLoanActionId, UnitLoanActionObservation,
    UnitLoanConsumerRetirement, UnitLoanForwardBranch, UnitLoanForwardHandoff,
};
pub use loan_auxiliary_retirement::{
    UnitLoanAuxiliaryRetirement, UnitLoanDistinctAuxiliaryRetirement,
};
pub use loan_auxiliary_sequence_regions::{
    UnitBorrowAuxiliarySequenceRegion, UnitLoanAuxiliarySequenceUse,
};
pub use loan_distinct_auxiliary_binding_regions::UnitBorrowDistinctAuxiliaryBindingRegion;
pub use loan_later_auxiliary_binding_regions::UnitBorrowLaterAuxiliaryBindingRegion;

mod loan;
pub use loan::{
    UnitBorrowBindingTransportFact, UnitBorrowCallHandoffFact, UnitBorrowCallPrefixEndFact,
    UnitBorrowCallTransportFact, UnitBorrowForwardTransportFact, UnitDirectCallLoanEndFact,
    UnitExclusiveCallPrefixEndFact, UnitLoanFact, UnitLoanTarget,
};

mod analysis;
mod binding;
mod capture;
mod constant;
#[cfg(test)]
mod constants_tests;
mod construction;
pub(super) mod contracts;
mod iteration;
mod iteration_validation;
mod map_iteration;
mod map_iteration_validation;
mod product;
pub use map_iteration::*;
#[cfg(test)]
mod iteration_validation_tests;
pub use iteration::*;
mod dataflow;
mod field_replace;
#[cfg(test)]
mod formed_tests;
#[cfg(test)]
mod materialization_tests;
mod non_null_assertion;
mod nullable_when;
pub use nullable_when::*;
mod owner_transport;
mod owner_transport_validation;
mod ownership_primitive;
pub use owner_transport::*;
#[cfg(test)]
mod owner_transport_tests;
#[cfg(test)]
mod pending_temporary_tests;
mod receiver;

mod place;
pub use place::UnitOwnershipPlace;

pub use binding::UnitOwnershipBindingDescriptor;
pub use capture::{UnitClosureCaptureDescriptor, UnitClosureCaptureSource, UnitClosureDescriptor};
pub use constant::{
    CompilationUnitConstantOwnership, ConstEnabledOwnedUnit, UnitConstantMaterializationPlan,
    UnitShortCircuitPlan, UnitShortCircuitRhs, check_compilation_unit_constant_ownership,
};
pub use construction::{
    UnitConstructionDeliveryEffect, UnitConstructionOwnershipPlan,
    UnitConstructionRootDropObligation,
};
pub use non_null_assertion::UnitNonNullAssertionOwnershipPlan;
pub use receiver::{
    UnitCallReceiverOwnershipContract, UnitConditionalReceiverDeliveryFact,
    UnitDelegationOwnershipPlan, UnitReceiverCallPrefixEndFact, UnitReceiverCallReturnEndFact,
    UnitReceiverOwnershipFact, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
};

#[cfg(test)]
use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    source::SourceMap,
    type_checking::TypeEnvironment,
};
use std::sync::Arc;

use crate::{
    diagnostic::{Diagnostic, Severity},
    name_resolution::{DeclarationId, UnitSymbolId},
    source::Span,
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, UnitExpressionId, UnitStatementId, UnitTypeId,
        ValidatedCompilationUnitTypes,
    },
};

use super::{
    LoanKind, OwnershipBindingKind, OwnershipCheckingError, OwnershipDeferredReason,
    RcOwnershipEffectKind, Transferability, UnitStringCloneLoanEndFact, UnitStringOwnershipEffect,
};

/// typed call argument 在 ownership 阶段采用的规范契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitCallArgumentOwnershipKind {
    /// 向 Value 参数交付 owned copy 或 move；具体效果由后续数据流决定。
    Value,
    /// 要求后续 body-local checker 在同步调用期间建立 shared loan。
    SharedLoan,
    /// 要求后续 body-local checker 在同步调用期间建立 exclusive loan。
    ExclusiveLoan,
}

/// 一个 source-qualified call argument 的 ownership 输入契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCallArgumentOwnershipContract {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    parameter_index: usize,
    parameter_type: UnitTypeId,
    category: ExpressionCategory,
    kind: UnitCallArgumentOwnershipKind,
    crosses_thread: bool,
    argument_span: Span,
    call_span: Span,
    parameter_span: Option<Span>,
    loan_begin_span: Span,
}

impl UnitCallArgumentOwnershipContract {
    /// 返回所属 call expression。
    #[must_use]
    pub const fn call(self) -> UnitExpressionId {
        self.call
    }

    /// 返回源码实参 expression。
    #[must_use]
    pub const fn argument(self) -> UnitExpressionId {
        self.argument
    }

    /// 返回声明顺序参数下标。
    #[must_use]
    pub const fn parameter_index(self) -> usize {
        self.parameter_index
    }

    /// 返回 callable 实例化后的参数类型。
    #[must_use]
    pub const fn parameter_type(self) -> UnitTypeId {
        self.parameter_type
    }

    /// 返回类型阶段确认的 place/temporary 类别。
    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    /// 返回 Value/shared-loan/exclusive-loan 契约。
    #[must_use]
    pub const fn kind(self) -> UnitCallArgumentOwnershipKind {
        self.kind
    }

    /// 返回参数是否由 compiler-bound effect 跨线程交付。
    #[must_use]
    pub const fn crosses_thread(self) -> bool {
        self.crosses_thread
    }

    /// 返回实参值表达式范围。
    #[must_use]
    pub const fn argument_span(self) -> Span {
        self.argument_span
    }

    /// 返回同步 call 的完整表达式范围。
    #[must_use]
    pub const fn call_span(self) -> Span {
        self.call_span
    }

    /// 返回源码 callable 参数声明范围；external/function-value target 没有源码参数范围。
    #[must_use]
    pub const fn parameter_span(self) -> Option<Span> {
        self.parameter_span
    }

    /// 返回 loan 的真实起点；Inout 使用调用点 `&`，其余使用 operand。
    #[must_use]
    pub const fn loan_begin_span(self) -> Span {
        self.loan_begin_span
    }
}

/// 向 Value 参数交付值时的实际所有权效果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitValueDeliveryKind {
    /// `Copyable` place 按值复制，源仍可用。
    Copy,
    /// MoveOnly owned place 被移动，源随后不可用。
    Move,
    /// 本次求值产生的 temporary 被直接交付。
    Temporary,
}

/// Value delivery 的可追溯来源。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitValueDeliverySource {
    /// 名称、字段或具名容器 element place。
    Place(UnitOwnershipPlace),
    /// 直接产生值的 temporary。
    Temporary(UnitExpressionId),
    /// 由 owner 支撑、但本身不是可移动 place 的 payload/element projection。
    BorrowedProjection {
        /// 保持投影值存活的 owner expression。
        owner: UnitExpressionId,
    },
}

/// 一个已经由 body-local 数据流确认的 Value argument delivery。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitValueDeliveryFact {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    source: UnitValueDeliverySource,
    kind: UnitValueDeliveryKind,
    span: Span,
    parameter_span: Option<Span>,
}

/// 一个 source-qualified intrinsic `Rc<T>` ownership effect。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitRcOwnershipEffect {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    payload_type: UnitTypeId,
    kind: RcOwnershipEffectKind,
}

impl UnitRcOwnershipEffect {
    pub(super) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        payload_type: UnitTypeId,
        kind: RcOwnershipEffectKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            payload_type,
            kind,
        }
    }

    /// 返回产生结果的 source-qualified expression。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回被读取但不消费的 shared-owner receiver。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回 control block payload 的 unit-global 类型。
    #[must_use]
    pub const fn payload_type(self) -> UnitTypeId {
        self.payload_type
    }

    /// 返回 retain 或 payload borrow effect。
    #[must_use]
    pub const fn kind(self) -> RcOwnershipEffectKind {
        self.kind
    }
}

impl UnitValueDeliveryFact {
    pub(super) const fn new(
        call: UnitExpressionId,
        argument: UnitExpressionId,
        source: UnitValueDeliverySource,
        kind: UnitValueDeliveryKind,
        span: Span,
        parameter_span: Option<Span>,
    ) -> Self {
        Self {
            call,
            argument,
            source,
            kind,
            span,
            parameter_span,
        }
    }

    /// 返回所属 call。
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }

    /// 返回源码实参。
    #[must_use]
    pub const fn argument(&self) -> UnitExpressionId {
        self.argument
    }

    /// 返回 place；temporary/borrowed projection 为 `None`。
    #[must_use]
    pub const fn place(&self) -> Option<&UnitOwnershipPlace> {
        match &self.source {
            UnitValueDeliverySource::Place(place) => Some(place),
            UnitValueDeliverySource::Temporary(_)
            | UnitValueDeliverySource::BorrowedProjection { .. } => None,
        }
    }

    /// 返回完整 delivery source。
    #[must_use]
    pub const fn source(&self) -> &UnitValueDeliverySource {
        &self.source
    }

    /// 返回 copy/move/temporary 效果。
    #[must_use]
    pub const fn kind(&self) -> UnitValueDeliveryKind {
        self.kind
    }

    /// 返回交付操作数的源码范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// 返回被选择源码参数的声明位置。
    #[must_use]
    pub const fn parameter_span(&self) -> Option<Span> {
        self.parameter_span
    }
}

/// unit ownership 仍无法发布完整 drop plan 的显式 recovery 边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitOwnershipDeferredFact {
    expression: UnitExpressionId,
    reason: OwnershipDeferredReason,
}

impl UnitOwnershipDeferredFact {
    pub(super) const fn new(expression: UnitExpressionId, reason: OwnershipDeferredReason) -> Self {
        Self { expression, reason }
    }

    /// 返回被延后的 source-qualified expression。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回未封闭的所有权类别。
    #[must_use]
    pub const fn reason(self) -> OwnershipDeferredReason {
        self.reason
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnitOwnershipProvenance {
    typed_analysis_owner: Arc<()>,
    analysis_owner: Arc<()>,
    requires_constant_capability: bool,
    typed_basic_is_valid: bool,
    typed_const_is_valid: bool,
}

/// SPEC-0198 的 recovery compilation-unit ownership product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitOwnership {
    #[cfg(feature = "d10-catalog-experiment")]
    source_collection_ledger: Option<d10_catalog_experiment::SourceCollectionLedger>,
    no_carry: loop_no_carry::CompletedNoCarry,
    pub(super) callable_provenance: super::callable_provenance::UnitCallableFacts,
    pub(super) borrow_results: super::borrow_result::UnitBorrowResults,
    pub(super) borrow_return_origins:
        Vec<super::BorrowReturnOriginFact<UnitExpressionId, UnitLoanTarget>>,
    iteration_conditional_scopes: Vec<(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)>,
    iteration_owner_scopes: Vec<(
        UnitStatementId,
        crate::ownership_checking::UnitDropTarget,
        bool,
    )>,
    iteration_loan_ends: Vec<(UnitDropPoint, UnitIterationCleanupAction)>,
    iterations: Vec<UnitIterationOwnershipPlan>,
    map_iterations: Vec<UnitMapIterationOwnershipPlan>,
    map_iteration_templates: Vec<UnitMapIterationOwnershipPlan>,
    map_iteration_inputs: Vec<crate::type_checking::UnitMapIterationDescriptor>,
    iteration_templates: Vec<UnitIterationOwnershipPlan>,
    iteration_required_exits: Vec<(
        crate::type_checking::UnitStatementId,
        UnitIterationExitKind,
        UnitDropPoint,
    )>,
    short_circuits: Option<Vec<constant::UnitShortCircuitPlan>>,
    constant_materializations: Option<Vec<constant::UnitConstantMaterializationPlan>>,
    non_null_assertions: Vec<UnitNonNullAssertionOwnershipPlan>,
    nullable_whens: Vec<UnitNullableWhenOwnershipPlan>,
    ownership_primitives: Vec<super::UnitOwnershipPrimitiveOwnershipPlan>,
    field_replacements: Vec<super::UnitFieldReplaceOwnershipPlan>,
    field_replacement_types:
        std::collections::BTreeMap<UnitExpressionId, field_replace::FieldReplacementType>,
    provenance: UnitOwnershipProvenance,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<UnitOwnershipBindingDescriptor>,
    formed_owned_locals: Vec<UnitSymbolId>,
    call_argument_contracts: Vec<UnitCallArgumentOwnershipContract>,
    call_receiver_contracts: Vec<UnitCallReceiverOwnershipContract>,
    receiver_facts: Vec<UnitReceiverOwnershipFact>,
    conditional_receiver_deliveries: Vec<UnitConditionalReceiverDeliveryFact>,
    delegations: Vec<UnitDelegationOwnershipPlan>,
    owner_observations: Vec<UnitOwnerObservation>,
    receiver_call_prefix_ends: Vec<UnitReceiverCallPrefixEndFact>,
    receiver_call_return_ends: Vec<UnitReceiverCallReturnEndFact>,
    exclusive_call_prefix_ends: Vec<UnitExclusiveCallPrefixEndFact>,
    loan_action_observations: Vec<UnitLoanActionObservation>,
    direct_call_loan_ends: Vec<UnitDirectCallLoanEndFact>,
    class_result_argument_unwinds: Vec<UnitDirectCallLoanEndFact>,
    borrow_call_handoffs: Vec<UnitBorrowCallHandoffFact>,
    borrow_call_transports: Vec<UnitBorrowCallTransportFact>,
    borrow_binding_transports: Vec<UnitBorrowBindingTransportFact>,
    borrow_forward_transports: Vec<UnitBorrowForwardTransportFact>,
    borrow_call_prefix_ends: Vec<UnitBorrowCallPrefixEndFact>,
    loans: Vec<UnitLoanFact>,
    value_deliveries: Vec<UnitValueDeliveryFact>,
    rc_effects: Vec<UnitRcOwnershipEffect>,
    string_effects: Vec<UnitStringOwnershipEffect>,
    string_clone_loan_ends: Vec<UnitStringCloneLoanEndFact>,
    construction_plans: Vec<UnitConstructionOwnershipPlan>,
    drops: Vec<UnitDropFact>,
    conditional_receiver_drops: Vec<UnitConditionalReceiverDropFact>,
    captures: Vec<UnitClosureCaptureDescriptor>,
    closures: Vec<UnitClosureDescriptor>,
    transferabilities: Vec<Transferability>,
    deferred: Vec<UnitOwnershipDeferredFact>,
}

/// 不可伪造的无错误、drop plan 完整的 compilation-unit ownership product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCompilationUnitOwnership(CompilationUnitOwnership);

impl ValidatedCompilationUnitOwnership {
    /// 返回 recovery product 的只读视图。
    #[must_use]
    pub const fn ownership(&self) -> &CompilationUnitOwnership {
        &self.0
    }

    /// 解包 recovery product。
    #[must_use]
    pub fn into_ownership(self) -> CompilationUnitOwnership {
        self.0
    }
}

pub use analysis::check_compilation_unit_ownership;
pub(crate) use analysis::check_compilation_unit_ownership_recovering;

#[cfg(feature = "loan-observation-test-support")]
pub(super) mod loan_observation_test_support;

#[cfg(feature = "d10-catalog-experiment")]
pub mod d10_catalog_experiment;
