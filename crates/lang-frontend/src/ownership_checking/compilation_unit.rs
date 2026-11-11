//! SPEC-0198 compilation-unit 所有权产物、身份门禁与 body-local call 数据流。
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

use std::sync::Arc;

use crate::{
    diagnostic::{Diagnostic, Severity},
    name_resolution::{
        CompilationUnitNames, DeclarationId, SourceUnitInput, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, TypeEnvironment, UnitExpressionId,
        UnitStatementId, UnitTypeId, ValidatedCompilationUnitTypes,
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
    field_replacement_types: Vec<(
        UnitExpressionId,
        UnitSymbolId,
        UnitTypeId,
        UnitSymbolId,
        UnitTypeId,
    )>,
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

impl CompilationUnitOwnership {
    /// Whole-callable narrow owner transport observations; Unsupported is not a language error.
    #[must_use]
    pub fn owner_observations(&self) -> &[UnitOwnerObservation] {
        &self.owner_observations
    }

    /// 已检查的 source-qualified 一级字段原子置换。
    #[must_use]
    pub fn field_replacements(&self) -> &[super::UnitFieldReplaceOwnershipPlan] {
        &self.field_replacements
    }
    /// 按调用 identity 查询独立字段提交能力。
    #[must_use]
    pub fn field_replacement(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitFieldReplaceOwnershipPlan> {
        self.field_replacements
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 正常执行前缀后已获准的 source-qualified root 原子 commit。
    #[must_use]
    pub fn ownership_primitives(&self) -> &[super::UnitOwnershipPrimitiveOwnershipPlan] {
        &self.ownership_primitives
    }
    /// 按调用 identity 查询正常 root commit。
    #[must_use]
    pub fn ownership_primitive(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&super::UnitOwnershipPrimitiveOwnershipPlan> {
        self.ownership_primitives
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 返回按 source-qualified assertion identity 排序的合法提取计划。
    #[must_use]
    pub fn non_null_assertions(&self) -> &[UnitNonNullAssertionOwnershipPlan] {
        &self.non_null_assertions
    }
    /// 查询仅成功边转移、null 边 Abort 的 ownership plan。
    #[must_use]
    pub fn non_null_assertion(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitNonNullAssertionOwnershipPlan> {
        self.non_null_assertions
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    fn new(
        typed: &CompilationUnitTypes,
        bindings: Vec<UnitOwnershipBindingDescriptor>,
        call_argument_contracts: Vec<UnitCallArgumentOwnershipContract>,
        call_receiver_contracts: Vec<UnitCallReceiverOwnershipContract>,
        capture: capture::Analysis,
        dataflow: dataflow::Analysis,
    ) -> Self {
        let successful = dataflow.diagnostics.is_empty() && typed.has_valid_type_facts();
        let mut capture = capture;
        capture.filter_unvisited(&dataflow.visited_lambdas, dataflow.short_circuits.is_some());
        let captures = if successful {
            capture.captures
        } else {
            Vec::new()
        };
        let delegations = if successful {
            typed
                .signatures()
                .delegations()
                .iter()
                .filter(|plan| !plan.forwarders().is_empty())
                .map(|plan| {
                    UnitDelegationOwnershipPlan::new(
                        plan.owner(),
                        plan.target(),
                        plan.delegation_span(),
                        plan.forwarders()
                            .iter()
                            .map(|forwarder| forwarder.requirement())
                            .collect(),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            iteration_conditional_scopes: dataflow.iteration_conditional_scopes,
            iteration_owner_scopes: dataflow.iteration_owner_scopes,
            iteration_loan_ends: dataflow.iteration_loan_ends,
            iterations: if successful && dataflow.deferred.is_empty() {
                dataflow.iterations
            } else {
                Vec::new()
            },
            map_iterations: if successful && dataflow.deferred.is_empty() {
                dataflow.map_iterations
            } else {
                Vec::new()
            },
            map_iteration_templates: dataflow.map_iteration_templates,
            map_iteration_inputs: typed.map_iterations().to_vec(),
            iteration_templates: dataflow.iteration_templates,
            iteration_required_exits: dataflow.iteration_required_exits,
            short_circuits: (successful && dataflow.deferred.is_empty())
                .then_some(dataflow.short_circuits)
                .flatten(),
            constant_materializations: (successful
                && dataflow.deferred.is_empty()
                && typed.constants().is_some())
            .then_some(dataflow.constant_materializations),
            borrow_results: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_results
            } else {
                Default::default()
            },
            borrow_return_origins: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_return_origins
            } else {
                Vec::new()
            },
            callable_provenance: super::callable_provenance::UnitCallableFacts {
                entered_lambdas: if successful {
                    dataflow.visited_lambdas
                } else {
                    Default::default()
                },
                ..dataflow.callable_provenance
            },
            provenance: UnitOwnershipProvenance {
                typed_analysis_owner: Arc::clone(typed.analysis_owner()),
                typed_basic_is_valid: typed.has_valid_basic_types(),
                typed_const_is_valid: typed.has_valid_constant_types(),
                analysis_owner: Arc::new(()),
                requires_constant_capability: typed
                    .constants()
                    .is_some_and(|facts| !facts.declarations().is_empty()),
            },
            non_null_assertions: dataflow.non_null_assertions,
            nullable_whens: dataflow.nullable_whens,
            field_replacement_types: Self::field_replacement_types(
                typed,
                &dataflow.field_replacements,
            ),
            field_replacements: if successful && dataflow.deferred.is_empty() {
                dataflow.field_replacements
            } else {
                Vec::new()
            },
            ownership_primitives: if successful && dataflow.deferred.is_empty() {
                dataflow.ownership_primitives
            } else {
                Vec::new()
            },
            diagnostics: dataflow.diagnostics,
            bindings,
            formed_owned_locals: if successful && dataflow.deferred.is_empty() {
                dataflow.formed_owned_locals
            } else {
                Vec::new()
            },
            call_argument_contracts,
            call_receiver_contracts,
            receiver_facts: dataflow.receiver_facts,
            conditional_receiver_deliveries: dataflow.conditional_receiver_deliveries,
            delegations,
            string_clone_loan_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.string_clone_loan_ends
            } else {
                Vec::new()
            },
            string_effects: if successful && dataflow.deferred.is_empty() {
                typed
                    .string_operations()
                    .iter()
                    .copied()
                    .filter(|operation| {
                        dataflow.loans.iter().any(|loan| {
                            loan.call() == operation.expression()
                                && loan.argument() == operation.receiver()
                        })
                    })
                    .map(UnitStringOwnershipEffect::new)
                    .collect()
            } else {
                Vec::new()
            },
            owner_observations: owner_transport::publish_observations(
                dataflow.owner_observations,
                successful,
                dataflow.deferred.is_empty(),
            ),
            receiver_call_prefix_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.receiver_call_prefix_ends
            } else {
                Vec::new()
            },
            receiver_call_return_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.receiver_call_return_ends
            } else {
                Vec::new()
            },
            exclusive_call_prefix_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.exclusive_call_prefix_ends
            } else {
                Vec::new()
            },
            borrow_call_prefix_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_call_prefix_ends
            } else {
                Vec::new()
            },
            borrow_forward_transports: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_forward_transports
            } else {
                Vec::new()
            },
            borrow_binding_transports: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_binding_transports
            } else {
                Vec::new()
            },
            borrow_call_transports: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_call_transports
            } else {
                Vec::new()
            },
            borrow_call_handoffs: if successful && dataflow.deferred.is_empty() {
                dataflow.borrow_call_handoffs
            } else {
                Vec::new()
            },
            loan_action_observations: if successful && dataflow.deferred.is_empty() {
                dataflow.loan_action_observations
            } else {
                Vec::new()
            },
            direct_call_loan_ends: if successful && dataflow.deferred.is_empty() {
                dataflow.direct_call_loan_ends
            } else {
                Vec::new()
            },
            loans: dataflow.loans,
            value_deliveries: dataflow.value_deliveries,
            rc_effects: dataflow.rc_effects,
            construction_plans: dataflow.construction_plans,
            drops: dataflow.drops,
            conditional_receiver_drops: dataflow.conditional_receiver_drops,
            captures,
            closures: capture.closures,
            transferabilities: capture.transferabilities,
            deferred: dataflow.deferred,
        }
    }

    /// 判断本产物是否来自指定 validated typed unit；克隆 typed/product 保留身份。
    #[must_use]
    pub fn is_compatible_with(&self, typed: &ValidatedCompilationUnitTypes) -> bool {
        Arc::ptr_eq(
            &self.provenance.typed_analysis_owner,
            typed.types().analysis_owner(),
        )
    }

    /// 判断两个 ownership product 是否来自同一次分析。
    #[must_use]
    pub fn is_same_analysis(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.provenance.analysis_owner,
            &other.provenance.analysis_owner,
        )
    }

    /// 返回稳定 source/symbol 顺序的参数能力。
    #[must_use]
    pub fn bindings(&self) -> &[UnitOwnershipBindingDescriptor] {
        &self.bindings
    }

    /// 返回某条分析路径正常完成初始化的具名 val/var local，按 source/symbol 排序去重。
    /// 仅记录形成历史，不证明必执行、当前存活、owner 权限或 nullable proof。
    #[must_use]
    pub fn formed_owned_locals(&self) -> &[UnitSymbolId] {
        &self.formed_owned_locals
    }

    /// 查询 source-qualified 参数 binding 能力。
    #[must_use]
    pub fn binding_kind(&self, symbol: UnitSymbolId) -> Option<OwnershipBindingKind> {
        self.bindings
            .binary_search_by_key(&symbol, |binding| binding.symbol())
            .ok()
            .map(|index| self.bindings[index].kind())
    }

    /// 返回稳定排序的所有权诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 返回稳定 source/call/argument 顺序的 ownership call contracts。
    #[must_use]
    pub fn call_argument_contracts(&self) -> &[UnitCallArgumentOwnershipContract] {
        &self.call_argument_contracts
    }

    /// 返回指定同步 call 的源码顺序 ownership contracts。
    pub fn call_argument_contracts_for(
        &self,
        call: UnitExpressionId,
    ) -> impl Iterator<Item = &UnitCallArgumentOwnershipContract> {
        self.call_argument_contracts
            .iter()
            .filter(move |contract| contract.call() == call)
    }

    /// 返回稳定 source/call 顺序的 receiver ownership contracts。
    #[must_use]
    pub fn call_receiver_contracts(&self) -> &[UnitCallReceiverOwnershipContract] {
        &self.call_receiver_contracts
    }

    /// 查询指定 member call 的 receiver contract。
    #[must_use]
    pub fn call_receiver_contract(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitCallReceiverOwnershipContract> {
        self.call_receiver_contracts
            .iter()
            .copied()
            .find(|contract| contract.call() == call)
    }

    /// 返回稳定 source/call 顺序的成功 receiver ownership facts。
    #[must_use]
    pub fn receiver_facts(&self) -> &[UnitReceiverOwnershipFact] {
        &self.receiver_facts
    }

    /// 查询指定 member call 的 receiver ownership fact。
    #[must_use]
    pub fn receiver_fact(&self, call: UnitExpressionId) -> Option<&UnitReceiverOwnershipFact> {
        self.receiver_facts.iter().find(|fact| fact.call() == call)
    }

    /// 返回源码/调用顺序稳定的 `StaticSelf` Value receiver 条件交付事实。
    #[must_use]
    pub fn conditional_receiver_deliveries(&self) -> &[UnitConditionalReceiverDeliveryFact] {
        &self.conditional_receiver_deliveries
    }

    /// 查询指定 member call 的 `StaticSelf` Value receiver 条件交付事实。
    #[must_use]
    pub fn conditional_receiver_delivery(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitConditionalReceiverDeliveryFact> {
        self.conditional_receiver_deliveries
            .iter()
            .copied()
            .find(|fact| fact.call() == call)
    }

    /// 返回 Borrow-only 委托的 outer receiver/delegate field shared-loan plans。
    #[must_use]
    pub fn delegations(&self) -> &[UnitDelegationOwnershipPlan] {
        &self.delegations
    }

    /// String.clone receiver 正常完成的独立 planner 观察，保持本表遍历顺序。
    /// 不包含普通同名方法、size、未被 planner 遍历或 receiver 未正常完成的 clone。
    /// 错误或 deferred 时整表撤销；不承诺常量条件或全 CFG 的不可达裁剪。
    /// 不是动态执行 trace，也不授予与 drops 或其他 loan-end 表的统一顺序。
    #[must_use]
    pub fn string_clone_loan_ends(&self) -> &[UnitStringCloneLoanEndFact] {
        &self.string_clone_loan_ends
    }

    /// 普通直接调用 Borrow 实参的实际结束事件，保持 planner 的结束动作顺序。
    /// 仅覆盖 owned-result、无 receiver 的 declaration/symbol call：operand 为可分组的
    /// owned val/var 或具名参数根、已完成求值的 String 字面量（含插值）、已验证构造形成的完整 MoveOnly
    /// temporary，或沿 typed Field/Group 链证明的 MoveOnly temporary 父字段。
    /// 构造与字段 target 均须匹配实际 owner。排除普通工厂调用结果、借用绑定、provider
    /// 来源/绑定、其他投影、借用结果、来源移交、Inout、receiver 与 capture end。
    /// 本轮任一 provider 使用的名称根均保守排除，不按调用位置扩大准入。
    /// 空列表不证明其它种类的 loan 仍活跃或已经结束。
    #[must_use]
    pub fn direct_call_loan_ends(&self) -> &[UnitDirectCallLoanEndFact] {
        &self.direct_call_loan_ends
    }

    /// 真实 source token 跨 inner return 的有界同步 consumer 运输；不是完整 loan-end 流。
    #[must_use]
    pub fn borrow_call_transports(&self) -> &[UnitBorrowCallTransportFact] {
        &self.borrow_call_transports
    }

    /// 真实 source token 从直接或透明 Group initializer 运输到 local binding 实际终止；不是完整流。
    #[must_use]
    pub fn borrow_binding_transports(&self) -> &[UnitBorrowBindingTransportFact] {
        &self.borrow_binding_transports
    }

    /// 真实参数 source token 跨 inner return 交接给当前函数返回；不是 source 结束。
    #[must_use]
    pub fn borrow_forward_transports(&self) -> &[UnitBorrowForwardTransportFact] {
        &self.borrow_forward_transports
    }

    /// 已完成的具名 Shared 根来源借用返回交接；不包含完整 loan-end 流。
    #[must_use]
    pub fn borrow_call_handoffs(&self) -> &[UnitBorrowCallHandoffFact] {
        &self.borrow_call_handoffs
    }

    /// 活跃 initializer 被 return 放弃时，已求值来源 token 的结束观察片。
    #[must_use]
    pub fn borrow_call_prefix_ends(&self) -> &[UnitBorrowCallPrefixEndFact] {
        &self.borrow_call_prefix_ends
    }

    /// 无 receiver、owned-result 的 declaration/symbol/external 调用中，Exclusive
    /// owned 名称根实参因 return/break/continue 放弃调用前缀的实际结束事件。
    /// 排除借用绑定、provider 来源/绑定、投影、临时值和借用结果。
    /// 保持真实动作顺序；空列表不证明其它 loan 的状态，abort 不产生 unwind。
    #[must_use]
    pub fn exclusive_call_prefix_ends(&self) -> &[UnitExclusiveCallPrefixEndFact] {
        &self.exclusive_call_prefix_ends
    }

    /// owned/Borrow/Inout 具名根、字段、具体 this 及首层直接根/字段来源借用绑定的 Shared receiver 实际 unwind 观测。
    /// 只观察 return/break/continue 退休的子 token，不结束绑定的来源 lease 或父 loan。
    /// 不提供正常返回、reservation 或完整 receiver loan-end 流。
    #[must_use]
    pub fn receiver_call_prefix_ends(&self) -> &[UnitReceiverCallPrefixEndFact] {
        &self.receiver_call_prefix_ends
    }

    /// owned/Borrow/Inout 具名根、字段、具体 this 及首层直接根/字段来源借用绑定的 Shared receiver 正常退休 token。
    /// 绑定 operand 与 canonical source lease 身份分开，不把子结束当作父 binding 结束。
    /// 不提供跨事务顺序、完整 receiver 流或动态执行次数；前缀退出另行查询。
    #[must_use]
    pub fn receiver_call_return_ends(&self) -> &[UnitReceiverCallReturnEndFact] {
        &self.receiver_call_return_ends
    }

    /// 返回源码/调用顺序稳定的有效同步 loans。
    #[must_use]
    pub fn loans(&self) -> &[UnitLoanFact] {
        &self.loans
    }

    /// 返回由指定 call 建立的 loan 描述符，按所属 call identity 筛选。
    /// 此查询不表示实际 loan-end 事件；借用结果的来源可延续到后续消费边界。
    pub fn loans_ending_at(&self, call: UnitExpressionId) -> impl Iterator<Item = &UnitLoanFact> {
        self.loans.iter().filter(move |loan| loan.call() == call)
    }

    /// 返回源码/调用顺序稳定的有效 Value deliveries。
    #[must_use]
    pub fn value_deliveries(&self) -> &[UnitValueDeliveryFact] {
        &self.value_deliveries
    }

    /// 返回已批准的 String 显式复制效果。
    #[must_use]
    pub fn string_effects(&self) -> &[UnitStringOwnershipEffect] {
        &self.string_effects
    }
    /// 查询已批准的 String 显式复制效果。
    #[must_use]
    pub fn string_effect(&self, expression: UnitExpressionId) -> Option<UnitStringOwnershipEffect> {
        self.string_effects
            .iter()
            .copied()
            .find(|effect| effect.expression() == expression)
    }

    /// 返回源码顺序稳定的 intrinsic Rc retain/payload-borrow effects。
    #[must_use]
    pub fn rc_effects(&self) -> &[UnitRcOwnershipEffect] {
        &self.rc_effects
    }

    /// 返回源码顺序稳定的 construction ordered-delivery/root plans。
    #[must_use]
    pub fn construction_plans(&self) -> &[UnitConstructionOwnershipPlan] {
        &self.construction_plans
    }

    /// 返回 source/control-flow 顺序稳定的 ASAP drop facts。
    #[must_use]
    pub fn drops(&self) -> &[UnitDropFact] {
        &self.drops
    }

    /// 返回源码/控制流顺序稳定的 `StaticSelf` Value receiver 条件析构事实。
    #[must_use]
    pub fn conditional_receiver_drops(&self) -> &[UnitConditionalReceiverDropFact] {
        &self.conditional_receiver_drops
    }

    /// 返回 lambda/source 顺序稳定的 capture 输入事实。
    #[must_use]
    pub fn captures(&self) -> &[UnitClosureCaptureDescriptor] {
        &self.captures
    }

    /// 返回指定 source-qualified lambda 的 capture 输入事实。
    pub fn captures_of(
        &self,
        lambda: UnitExpressionId,
    ) -> impl Iterator<Item = &UnitClosureCaptureDescriptor> {
        self.captures
            .iter()
            .filter(move |capture| capture.lambda() == lambda)
    }

    /// 查询具体 lambda environment 的能力事实。
    #[must_use]
    pub fn closure(&self, expression: UnitExpressionId) -> Option<UnitClosureDescriptor> {
        self.closures
            .iter()
            .copied()
            .find(|closure| closure.expression() == expression)
    }

    /// 返回 source/lambda 顺序稳定的 closure environment 能力事实。
    #[must_use]
    pub fn closures(&self) -> &[UnitClosureDescriptor] {
        &self.closures
    }

    /// 查询一个 unit-global 类型的结构化跨线程转移能力。
    #[must_use]
    pub fn transferability(&self, ty: UnitTypeId) -> Option<Transferability> {
        self.transferabilities.get(ty.index()).copied()
    }

    /// 返回阻止 validated codegen gate 的显式 recovery 边界。
    #[must_use]
    pub fn deferred(&self) -> &[UnitOwnershipDeferredFact] {
        &self.deferred
    }

    /// 无 ownership error、drop plan 完整且不含常量来源时才发布基础 codegen view。
    pub fn validate(self) -> Result<ValidatedCompilationUnitOwnership, Box<Self>> {
        let has_error = self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        if !self.provenance.typed_basic_is_valid
            || has_error
            || !self.deferred.is_empty()
            || self.provenance.requires_constant_capability
            || !self.ownership_primitives_are_valid()
            || !self.field_replacements_are_valid()
            || !self.iterations_are_valid()
        {
            Err(Box::new(self))
        } else {
            Ok(ValidatedCompilationUnitOwnership(self))
        }
    }
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

/// 建立 source-qualified compilation-unit ownership recovery product。
///
/// 当前发布 callable parameter bindings、call argument contracts、普通 call 与 intrinsic
/// container 的 loan/value deliveries、receiver loan/delivery 与 `StaticSelf` conditional delivery、
/// intrinsic Rc effects、constructor ordered delivery/root obligations、closure
/// capture/formation/Transferability，以及完整 body-local ASAP drop facts。
/// recovery product 可能携带诊断或显式 deferred drop 边界；调用 [`CompilationUnitOwnership::validate`]
/// 后才获得 codegen 可消费的 view。
pub fn check_compilation_unit_ownership(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<CompilationUnitOwnership, OwnershipCheckingError> {
    analysis::analyze(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )
}

/// 同一 producer 的诊断入口；上游资格在产物中继承，constant control 仍为普通保守遍历。
pub(crate) fn check_compilation_unit_ownership_recovering(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &CompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &CompilationUnitTypes,
) -> Result<CompilationUnitOwnership, OwnershipCheckingError> {
    analysis::analyze(sources, inputs, names, environment, typed, false)
}

#[cfg(feature = "loan-observation-test-support")]
pub(super) mod loan_observation_test_support;
