//! SPEC-0198 compilation-unit 所有权产物、身份门禁与 body-local call 数据流。

mod analysis;
mod binding;
mod capture;
mod constant;
#[cfg(test)]
mod constants_tests;
mod construction;
mod contracts;
mod dataflow;
#[cfg(test)]
mod materialization_tests;
mod non_null_assertion;
#[cfg(test)]
mod pending_temporary_tests;
mod receiver;

pub use binding::UnitOwnershipBindingDescriptor;
pub use capture::{UnitClosureCaptureDescriptor, UnitClosureCaptureSource, UnitClosureDescriptor};
pub use constant::{
    CompilationUnitConstantOwnership, ConstEnabledOwnedUnit, UnitConstantMaterializationPlan,
    check_compilation_unit_constant_ownership,
};
pub use construction::{
    UnitConstructionDeliveryEffect, UnitConstructionOwnershipPlan,
    UnitConstructionRootDropObligation,
};
pub use non_null_assertion::UnitNonNullAssertionOwnershipPlan;
pub use receiver::{
    UnitCallReceiverOwnershipContract, UnitConditionalReceiverDeliveryFact,
    UnitDelegationOwnershipPlan, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
    UnitReceiverOwnershipTarget,
};

use std::sync::Arc;

use crate::{
    diagnostic::{Diagnostic, Severity},
    name_resolution::{
        DeclarationId, SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, TypeEnvironment, UnitExpressionId, UnitItemId,
        UnitStatementId, UnitTypeId, ValidatedCompilationUnitTypes,
    },
};

use super::{
    ElementIndexIdentity, LoanKind, OwnershipBindingKind, OwnershipCheckingError,
    OwnershipDeferredReason, RcOwnershipEffectKind, Transferability,
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

    /// 返回源码 callable 参数声明范围；external/function-value/structural-component target
    /// 没有源码参数范围。
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

/// compilation-unit 中由根 binding、字段路径及可选 terminal element 组成的稳定 place。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitOwnershipPlace {
    root: UnitSymbolId,
    fields: Vec<UnitSymbolId>,
    element: Option<ElementIndexIdentity>,
}

impl UnitOwnershipPlace {
    pub(super) fn new(root: UnitSymbolId, fields: Vec<UnitSymbolId>) -> Self {
        Self {
            root,
            fields,
            element: None,
        }
    }

    pub(super) fn push_field(&mut self, field: UnitSymbolId) -> bool {
        if self.element.is_some() {
            return false;
        }
        self.fields.push(field);
        true
    }

    pub(super) fn push_element(&mut self, element: ElementIndexIdentity) -> bool {
        if self.element.is_some() {
            return false;
        }
        self.element = Some(element);
        true
    }

    /// 返回拥有该 place 的 source-qualified 根 binding。
    #[must_use]
    pub const fn root(&self) -> UnitSymbolId {
        self.root
    }

    /// 返回从根到目标的字段路径。
    #[must_use]
    pub fn fields(&self) -> &[UnitSymbolId] {
        &self.fields
    }

    /// 返回 terminal 顺序容器逻辑索引。
    #[must_use]
    pub const fn element(&self) -> Option<ElementIndexIdentity> {
        self.element
    }

    /// 返回该 place 是否精确表示根 binding 自身。
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.fields.is_empty() && self.element.is_none()
    }

    pub(super) fn overlaps(&self, other: &Self) -> bool {
        if self.root != other.root
            || !self
                .fields
                .iter()
                .zip(&other.fields)
                .all(|(left, right)| left == right)
        {
            return false;
        }
        if self.fields.len() != other.fields.len() {
            return true;
        }
        match (self.element, other.element) {
            (Some(left), Some(right)) => left.may_alias(right),
            (None, _) | (_, None) => true,
        }
    }
}

/// unit loan 的稳定目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitLoanTarget {
    /// 名称、字段或 terminal element place。
    Place(UnitOwnershipPlace),
    /// 延长到同步调用返回的 temporary。
    Temporary(UnitExpressionId),
}

/// 一次成功建立并在同步 call 返回时结束的 source-qualified loan。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanFact {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    target: UnitLoanTarget,
    kind: LoanKind,
    begin_span: Span,
    end_span: Span,
    parameter_span: Option<Span>,
}

impl UnitLoanFact {
    pub(super) const fn new(
        call: UnitExpressionId,
        argument: UnitExpressionId,
        target: UnitLoanTarget,
        kind: LoanKind,
        begin_span: Span,
        end_span: Span,
        parameter_span: Option<Span>,
    ) -> Self {
        Self {
            call,
            argument,
            target,
            kind,
            begin_span,
            end_span,
            parameter_span,
        }
    }

    /// 返回所属同步 call。
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }

    /// 返回建立 loan 的源码实参。
    #[must_use]
    pub const fn argument(&self) -> UnitExpressionId {
        self.argument
    }

    /// 返回 place 或 temporary 目标。
    #[must_use]
    pub const fn target(&self) -> &UnitLoanTarget {
        &self.target
    }

    /// 返回 shared/exclusive loan 种类。
    #[must_use]
    pub const fn kind(&self) -> LoanKind {
        self.kind
    }

    /// 返回 loan 生效位置。
    #[must_use]
    pub const fn begin_span(&self) -> Span {
        self.begin_span
    }

    /// 返回同步 call 结束位置。
    #[must_use]
    pub const fn end_span(&self) -> Span {
        self.end_span
    }

    /// 返回被选择源码参数的声明位置；external/function-value 没有该位置。
    #[must_use]
    pub const fn parameter_span(&self) -> Option<Span> {
        self.parameter_span
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

/// Phase 4 可消费的 source-qualified 析构边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitDropPoint {
    /// 最后一次读取或 replacement RHS 完成后。
    AfterExpression(UnitExpressionId),
    /// 非消费式 binary 已读完全部 operand、但结果仍存活时。
    AfterBinaryOperands(UnitExpressionId),
    /// 未使用 binding 建立或完整 statement 完成后。
    AfterStatement(UnitStatementId),
    /// 借用 temporary 在同步 call 返回后。
    CallReturn(UnitExpressionId),
    /// return / break / continue 控制转移边。
    ControlTransfer(UnitExpressionId),
    /// if / when 的特定 branch 正常离开边。
    BranchExit {
        /// 控制表达式。
        control: UnitExpressionId,
        /// 源码顺序的 branch 下标。
        branch: usize,
    },
    /// while / for 的零次或正常退出边。
    LoopExit(UnitStatementId),
    /// callable body 开始、参数 binding 建立之后。
    FunctionEntry(UnitItemId),
    /// lambda callable body 开始、参数 binding 建立之后。
    LambdaEntry(UnitExpressionId),
    /// 普通字段 replacement 的 RHS 正常完成、旧字段析构开始之前。
    BeforeReplacement(UnitExpressionId),
    /// element replacement 已提交新值之后。
    AfterReplacement(UnitExpressionId),
}

/// 一个需要唯一析构的 source-qualified 运行时值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitDropTarget {
    /// Value receiver 尚未被整体移动时由当前 callable 负责的根值。
    This(DeclarationId),
    /// 当前 callable 拥有的 named binding 值。
    Named(UnitSymbolId),
    /// 完整表达式产生的 anonymous temporary。
    Temporary(UnitExpressionId),
    /// replacement 前原 element value；payload 是 assignment expression。
    ReplacedElement(UnitExpressionId),
    /// replacement 前原 field value；字段身份必须与 assignment target 的 typed projection 一致。
    ReplacedField {
        /// assignment expression。
        assignment: UnitExpressionId,
        /// 被替换字段的 source-qualified symbol。
        field: UnitSymbolId,
    },
    /// `move` closure environment 中一个 owned MoveOnly capture。
    Captured {
        /// 拥有 environment 的 lambda。
        closure: UnitExpressionId,
        /// 被析构的捕获来源。
        source: UnitClosureCaptureSource,
    },
}

/// 一个确定的 source-qualified ASAP 析构事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDropFact {
    point: UnitDropPoint,
    target: UnitDropTarget,
    value_origin: Span,
}

/// interface default 的 `StaticSelf` Value receiver 析构义务。
///
/// Phase 4 必须先把 [`Self::receiver_type`] 实例化为具体 receiver；仅当具体类型为
/// MoveOnly 时才消费该事实并生成析构。该事实与无条件 [`UnitDropFact`] 分开发布，避免在
/// Copyable specialization 上误析构。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitConditionalReceiverDropFact {
    point: UnitDropPoint,
    owner: DeclarationId,
    receiver_type: UnitTypeId,
    value_origin: Span,
}

impl UnitConditionalReceiverDropFact {
    pub(super) const fn new(
        point: UnitDropPoint,
        owner: DeclarationId,
        receiver_type: UnitTypeId,
        value_origin: Span,
    ) -> Self {
        Self {
            point,
            owner,
            receiver_type,
            value_origin,
        }
    }

    /// 返回与普通 drop fact 相同的 source-qualified 控制流边界。
    #[must_use]
    pub const fn point(self) -> UnitDropPoint {
        self.point
    }

    /// 返回声明 `StaticSelf` receiver 的 interface owner。
    #[must_use]
    pub const fn owner(self) -> DeclarationId {
        self.owner
    }

    /// 返回必须由下游实例化的 `StaticSelf` 类型模板。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }

    /// 返回当前 receiver 值实例的建立位置。
    #[must_use]
    pub const fn value_origin(self) -> Span {
        self.value_origin
    }
}

impl UnitDropFact {
    pub(super) const fn new(
        point: UnitDropPoint,
        target: UnitDropTarget,
        value_origin: Span,
    ) -> Self {
        Self {
            point,
            target,
            value_origin,
        }
    }

    /// 返回析构发生的 source-qualified 控制流边界。
    #[must_use]
    pub const fn point(self) -> UnitDropPoint {
        self.point
    }

    /// 返回 named owner、temporary、旧 element 或 owned capture。
    #[must_use]
    pub const fn target(self) -> UnitDropTarget {
        self.target
    }

    /// 返回当前值实例的建立位置。
    #[must_use]
    pub const fn value_origin(self) -> Span {
        self.value_origin
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
}

/// SPEC-0198 的 recovery compilation-unit ownership product。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilationUnitOwnership {
    constant_materializations: Option<Vec<constant::UnitConstantMaterializationPlan>>,
    non_null_assertions: Vec<UnitNonNullAssertionOwnershipPlan>,
    provenance: UnitOwnershipProvenance,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<UnitOwnershipBindingDescriptor>,
    call_argument_contracts: Vec<UnitCallArgumentOwnershipContract>,
    call_receiver_contracts: Vec<UnitCallReceiverOwnershipContract>,
    receiver_facts: Vec<UnitReceiverOwnershipFact>,
    conditional_receiver_deliveries: Vec<UnitConditionalReceiverDeliveryFact>,
    delegations: Vec<UnitDelegationOwnershipPlan>,
    loans: Vec<UnitLoanFact>,
    value_deliveries: Vec<UnitValueDeliveryFact>,
    rc_effects: Vec<UnitRcOwnershipEffect>,
    construction_plans: Vec<UnitConstructionOwnershipPlan>,
    drops: Vec<UnitDropFact>,
    conditional_receiver_drops: Vec<UnitConditionalReceiverDropFact>,
    captures: Vec<UnitClosureCaptureDescriptor>,
    closures: Vec<UnitClosureDescriptor>,
    transferabilities: Vec<Transferability>,
    deferred: Vec<UnitOwnershipDeferredFact>,
}

impl CompilationUnitOwnership {
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
        let successful = dataflow.diagnostics.is_empty();
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
            constant_materializations: (successful
                && dataflow.deferred.is_empty()
                && typed.constants().is_some())
            .then_some(dataflow.constant_materializations),
            provenance: UnitOwnershipProvenance {
                typed_analysis_owner: Arc::clone(typed.analysis_owner()),
                analysis_owner: Arc::new(()),
                requires_constant_capability: typed
                    .constants()
                    .is_some_and(|facts| !facts.declarations().is_empty()),
            },
            non_null_assertions: dataflow.non_null_assertions,
            diagnostics: dataflow.diagnostics,
            bindings,
            call_argument_contracts,
            call_receiver_contracts,
            receiver_facts: dataflow.receiver_facts,
            conditional_receiver_deliveries: dataflow.conditional_receiver_deliveries,
            delegations,
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

    /// 返回源码/调用顺序稳定的有效同步 loans。
    #[must_use]
    pub fn loans(&self) -> &[UnitLoanFact] {
        &self.loans
    }

    /// 返回在指定 call 结束的 loans。
    pub fn loans_ending_at(&self, call: UnitExpressionId) -> impl Iterator<Item = &UnitLoanFact> {
        self.loans.iter().filter(move |loan| loan.call() == call)
    }

    /// 返回源码/调用顺序稳定的有效 Value deliveries。
    #[must_use]
    pub fn value_deliveries(&self) -> &[UnitValueDeliveryFact] {
        &self.value_deliveries
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
        if has_error || !self.deferred.is_empty() || self.provenance.requires_constant_capability {
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
    analysis::analyze(sources, inputs, names, environment, typed.types())
}
