use std::sync::Arc;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::Diagnostic,
    name_resolution::{NameResolution, SymbolId},
    source::{SourceId, Span},
    type_checking::{TypeId, TypedFile},
};

use super::ConstructionOwnershipPlan;
use super::RcOwnershipEffect;

/// 可由 Phase 3 精确识别的源码 place。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnershipPlace {
    root: SymbolId,
    fields: Vec<SymbolId>,
    element: Option<ElementIndexIdentity>,
}

impl OwnershipPlace {
    pub(crate) fn new(root: SymbolId, fields: Vec<SymbolId>) -> Self {
        Self {
            root,
            fields,
            element: None,
        }
    }

    pub(crate) fn push_field(&mut self, field: SymbolId) -> bool {
        if self.element.is_some() {
            return false;
        }
        self.fields.push(field);
        true
    }

    pub(crate) fn push_element(&mut self, element: ElementIndexIdentity) -> bool {
        if self.element.is_some() {
            return false;
        }
        self.element = Some(element);
        true
    }

    /// 返回唯一根绑定。
    #[must_use]
    pub const fn root(&self) -> SymbolId {
        self.root
    }

    /// 返回从根到叶的字段路径。
    #[must_use]
    pub fn fields(&self) -> &[SymbolId] {
        &self.fields
    }

    /// 返回 terminal 顺序容器逻辑索引；非 element place 返回 `None`。
    #[must_use]
    pub const fn element(&self) -> Option<ElementIndexIdentity> {
        self.element
    }

    /// 返回该 place 是否精确表示根绑定自身。
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.fields.is_empty() && self.element.is_none()
    }

    /// 判断两个 place 是否相同或具有 parent/child 前缀关系。
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
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

/// Phase 3 可证明的顺序容器逻辑索引身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElementIndexIdentity {
    /// 编译期可读取的 `Int` 整数字面量。
    Known(i128),
    /// 动态索引；与同一 container 的任意索引保守视为可能重叠。
    Unknown,
}

impl ElementIndexIdentity {
    /// 判断两个逻辑索引是否可能指向同一元素。
    #[must_use]
    pub const fn may_alias(self, other: Self) -> bool {
        !matches!((self, other), (Self::Known(left), Self::Known(right)) if left != right)
    }
}

/// 参数绑定在所有权阶段提供的能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipBindingKind {
    /// 当前 callable 拥有该参数值。
    Owned,
    /// 调用者保持 owner，当前 callable 只有 shared 能力。
    Shared,
    /// 调用者保持 owner，当前 callable 持有 exclusive 能力。
    Exclusive,
}

/// 一个源码参数的所有权能力事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnershipBindingDescriptor {
    symbol: SymbolId,
    kind: OwnershipBindingKind,
}

impl OwnershipBindingDescriptor {
    pub(crate) const fn new(symbol: SymbolId, kind: OwnershipBindingKind) -> Self {
        Self { symbol, kind }
    }

    /// 返回参数 symbol。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.symbol
    }

    /// 返回 owned/shared/exclusive 能力。
    #[must_use]
    pub const fn kind(self) -> OwnershipBindingKind {
        self.kind
    }
}

/// 同步调用期 loan 的种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoanKind {
    /// `Borrow` 参数建立的 shared loan。
    Shared,
    /// `Inout` 参数建立的 exclusive loan。
    Exclusive,
}

/// loan 的稳定目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoanTarget {
    /// 名称或字段 place。
    Place(OwnershipPlace),
    /// 延长到调用返回的 MoveOnly temporary。
    Temporary(ExpressionId),
}

/// 一次成功建立的 loan；实际终止路径由 OwnershipCheckedFile::loan_ends 描述。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoanFact {
    call: ExpressionId,
    argument: ExpressionId,
    target: LoanTarget,
    kind: LoanKind,
    begin_span: Span,
    end_span: Span,
}

impl LoanFact {
    pub(crate) const fn new(
        call: ExpressionId,
        argument: ExpressionId,
        target: LoanTarget,
        kind: LoanKind,
        begin_span: Span,
        end_span: Span,
    ) -> Self {
        Self {
            call,
            argument,
            target,
            kind,
            begin_span,
            end_span,
        }
    }

    /// 返回所属 call expression。
    #[must_use]
    pub const fn call(&self) -> ExpressionId {
        self.call
    }

    /// 返回建立 loan 的 argument operand。
    #[must_use]
    pub const fn argument(&self) -> ExpressionId {
        self.argument
    }

    /// 返回 place 或 temporary 目标。
    #[must_use]
    pub const fn target(&self) -> &LoanTarget {
        &self.target
    }

    /// 返回 shared/exclusive 种类。
    #[must_use]
    pub const fn kind(&self) -> LoanKind {
        self.kind
    }

    /// 返回 loan 生效位置。
    #[must_use]
    pub const fn begin_span(&self) -> Span {
        self.begin_span
    }

    /// 返回同步调用结束位置。
    #[must_use]
    pub const fn end_span(&self) -> Span {
        self.end_span
    }
}

/// Phase 4 可消费的析构边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPoint {
    /// 最后一次读取或 replacement RHS 完成后。
    AfterExpression(ExpressionId),
    /// 非消费式 binary 已读完全部 operand、但结果仍存活时。
    AfterBinaryOperands(ExpressionId),
    /// 未使用 binding 建立或完整 statement 完成后。
    AfterStatement(StatementId),
    /// 借用 temporary 在同步 call 返回后。
    CallReturn(ExpressionId),
    /// return / break / continue 控制转移边。
    ControlTransfer(ExpressionId),
    /// if / when 的特定 branch 正常离开边。
    BranchExit {
        /// 控制表达式。
        control: ExpressionId,
        /// 源码顺序的 branch 下标。
        branch: usize,
    },
    /// 一个 when alternative 匹配后、进入该 entry 的共享 body 之前。
    WhenAlternativeMatch {
        /// when 表达式。
        control: ExpressionId,
        /// 源码顺序的 entry 下标。
        entry: usize,
        /// 该 entry 内源码顺序的 alternative 下标。
        alternative: usize,
    },
    /// while / for 的零次或正常退出边。
    LoopExit(StatementId),
    /// 无 body callable 的参数建立之后。
    FunctionEntry(ItemId),
    /// element replacement 已提交新值之后。
    AfterReplacement(ExpressionId),
}

/// 一个需要唯一析构的运行时值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// 当前 callable 拥有的 named binding 值。
    Named(SymbolId),
    /// 完整表达式产生的 anonymous temporary。
    Temporary(ExpressionId),
    /// replacement 前原 element value；payload 是 assignment expression。
    ReplacedElement(ExpressionId),
    /// `move` closure environment 中一个 owned MoveOnly capture。
    Captured {
        /// 拥有 environment 的 lambda。
        closure: ExpressionId,
        /// 被析构的捕获来源。
        source: ClosureCaptureSource,
    },
}

/// 一个确定的 ASAP 析构事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropFact {
    point: DropPoint,
    target: DropTarget,
    value_origin: Span,
}

/// lambda environment 中一个捕获来源的稳定身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClosureCaptureSource {
    /// 解析后的词法 binding。
    Symbol(SymbolId),
    /// 显式 `this` 或规范化为同一 receiver 的无前缀字段引用。
    This,
}

/// closure 对捕获值持有的能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosureCaptureMode {
    /// 默认 lambda 延长 shared loan。
    Shared,
    /// `move` lambda 拥有独立 environment value。
    Owned,
}

/// 形成 closure environment 时发生的值效果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosureCaptureEffect {
    /// 建立 shared capture loan。
    Borrow,
    /// 将 `Copyable` 值复制进 owned environment。
    Copy,
    /// 将 MoveOnly 值移动进 owned environment。
    Move,
    /// 上游 deferred/error 类型阻止当前阶段确定效果。
    Unknown,
}

/// 一个 lambda 对一个解析后来源的捕获事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosureCaptureDescriptor {
    lambda: ExpressionId,
    source: ClosureCaptureSource,
    ty: TypeId,
    mode: ClosureCaptureMode,
    effect: ClosureCaptureEffect,
    reference_span: Span,
}

impl ClosureCaptureDescriptor {
    pub(crate) const fn new(
        lambda: ExpressionId,
        source: ClosureCaptureSource,
        ty: TypeId,
        mode: ClosureCaptureMode,
        effect: ClosureCaptureEffect,
        reference_span: Span,
    ) -> Self {
        Self {
            lambda,
            source,
            ty,
            mode,
            effect,
            reference_span,
        }
    }

    /// 返回拥有该 environment 的 lambda。
    #[must_use]
    pub const fn lambda(self) -> ExpressionId {
        self.lambda
    }

    /// 返回解析后的 binding 或 receiver 来源。
    #[must_use]
    pub const fn source(self) -> ClosureCaptureSource {
        self.source
    }

    /// 返回 capture 形成位置看到的规范化类型。
    #[must_use]
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// 返回 shared/owned capture mode。
    #[must_use]
    pub const fn mode(self) -> ClosureCaptureMode {
        self.mode
    }

    /// 返回 borrow/copy/move formation effect。
    #[must_use]
    pub const fn effect(self) -> ClosureCaptureEffect {
        self.effect
    }

    /// 返回首次触发该 capture 的源码引用范围。
    #[must_use]
    pub const fn reference_span(self) -> Span {
        self.reference_span
    }
}

/// 一个规范化类型或具体 closure value 的跨线程转移能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transferability {
    /// 已结构化证明可跨线程交付。
    Transferable,
    /// 当前类型明确不满足该能力。
    NotTransferable,
    /// 仍依赖后续 typed selection。
    Unknown,
    /// 源码类型或能力位置无效。
    Error,
}

/// 一个具体 lambda value 的 environment 与转移能力事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosureDescriptor {
    expression: ExpressionId,
    move_owned: bool,
    transferability: Transferability,
}

impl ClosureDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        move_owned: bool,
        transferability: Transferability,
    ) -> Self {
        Self {
            expression,
            move_owned,
            transferability,
        }
    }

    /// 返回 lambda expression identity。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回是否为显式 `move` lambda。
    #[must_use]
    pub const fn move_owned(self) -> bool {
        self.move_owned
    }

    /// 返回考虑具体 capture environment 后的转移能力。
    #[must_use]
    pub const fn transferability(self) -> Transferability {
        self.transferability
    }
}

impl DropFact {
    pub(crate) const fn new(point: DropPoint, target: DropTarget, value_origin: Span) -> Self {
        Self {
            point,
            target,
            value_origin,
        }
    }

    /// 返回析构发生的控制流边界。
    #[must_use]
    pub const fn point(self) -> DropPoint {
        self.point
    }

    /// 返回 named owner 或 temporary。
    #[must_use]
    pub const fn target(self) -> DropTarget {
        self.target
    }

    /// 返回当前值实例的建立位置。
    #[must_use]
    pub const fn value_origin(self) -> Span {
        self.value_origin
    }
}

/// SPEC-0029 明确保留到后续 Goal 的 place 类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipDeferredReason {
    /// 顺序容器 index place 等待 SPEC-0030。
    IndexPlace,
    /// 未具有静态参数契约的 instance member receiver。
    MemberReceiver,
}

/// 一个可查询的所有权 deferred 边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnershipDeferredFact {
    expression: ExpressionId,
    reason: OwnershipDeferredReason,
}

impl OwnershipDeferredFact {
    pub(crate) const fn new(expression: ExpressionId, reason: OwnershipDeferredReason) -> Self {
        Self { expression, reason }
    }

    /// 返回 deferred expression。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回封闭的后续工作类别。
    #[must_use]
    pub const fn reason(self) -> OwnershipDeferredReason {
        self.reason
    }
}

/// Phase 3 单文件所有权检查产物。
#[derive(Clone, Debug)]
pub struct OwnershipCheckedFile {
    non_null_assertions: Vec<super::NonNullAssertionOwnershipPlan>,
    loan_ends: Vec<LoanEndFact>,
    nullable_whens: Vec<super::NullableWhenOwnershipPlan>,
    source_id: SourceId,
    environment_owner: Arc<()>,
    typed_analysis_owner: Arc<()>,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<OwnershipBindingDescriptor>,
    loans: Vec<LoanFact>,
    drops: Vec<DropFact>,
    construction_plans: Vec<ConstructionOwnershipPlan>,
    rc_effects: Vec<RcOwnershipEffect>,
    captures: Vec<ClosureCaptureDescriptor>,
    closures: Vec<ClosureDescriptor>,
    transferabilities: Vec<Transferability>,
    deferred: Vec<OwnershipDeferredFact>,
}

pub(crate) struct OwnershipCheckedParts {
    pub(crate) non_null_assertions: Vec<super::NonNullAssertionOwnershipPlan>,
    pub(crate) loan_ends: Vec<LoanEndFact>,
    pub(crate) nullable_whens: Vec<super::NullableWhenOwnershipPlan>,
    pub(crate) bindings: Vec<OwnershipBindingDescriptor>,
    pub(crate) loans: Vec<LoanFact>,
    pub(crate) drops: Vec<DropFact>,
    pub(crate) construction_plans: Vec<ConstructionOwnershipPlan>,
    pub(crate) rc_effects: Vec<RcOwnershipEffect>,
    pub(crate) captures: Vec<ClosureCaptureDescriptor>,
    pub(crate) closures: Vec<ClosureDescriptor>,
    pub(crate) transferabilities: Vec<Transferability>,
    pub(crate) deferred: Vec<OwnershipDeferredFact>,
}

impl OwnershipCheckedFile {
    /// 返回按 assertion 身份排序、仅在无所有权诊断时发布的提取计划。
    #[must_use]
    pub fn non_null_assertions(&self) -> &[super::NonNullAssertionOwnershipPlan] {
        &self.non_null_assertions
    }
    /// 按 assertion 表达式身份查询成功转移与 null Abort 计划。
    #[must_use]
    pub fn non_null_assertion(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::NonNullAssertionOwnershipPlan> {
        self.non_null_assertions
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 返回实际控制流上的 loan 终止事实；同边先结束loan再执行drop。
    pub fn loan_ends(&self) -> &[LoanEndFact] {
        &self.loan_ends
    }
    /// 返回仅在无诊断时发布的 nullable ownership plans。
    pub fn nullable_whens(&self) -> &[super::NullableWhenOwnershipPlan] {
        &self.nullable_whens
    }
    /// 按 when 表达式身份查询 ownership plan。
    pub fn nullable_when(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::NullableWhenOwnershipPlan> {
        self.nullable_whens
            .iter()
            .find(|plan| plan.expression() == expression)
    }
    pub(crate) fn new(
        source_id: SourceId,
        environment_owner: Arc<()>,
        typed_analysis_owner: Arc<()>,
        diagnostics: Vec<Diagnostic>,
        parts: OwnershipCheckedParts,
    ) -> Self {
        Self {
            source_id,
            environment_owner,
            nullable_whens: parts.nullable_whens,
            non_null_assertions: parts.non_null_assertions,
            loan_ends: parts.loan_ends,
            typed_analysis_owner,
            diagnostics,
            bindings: parts.bindings,
            loans: parts.loans,
            drops: parts.drops,
            construction_plans: parts.construction_plans,
            rc_effects: parts.rc_effects,
            captures: parts.captures,
            closures: parts.closures,
            transferabilities: parts.transferabilities,
            deferred: parts.deferred,
        }
    }

    /// 返回输入源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// 返回本产物是否与名称、类型产物共享源码和显式环境身份。
    #[must_use]
    pub fn is_compatible_with(&self, names: &NameResolution, typed: &TypedFile) -> bool {
        self.source_id == names.source_id()
            && self.source_id == typed.source_id()
            && typed.is_compatible_with_names(names)
            && Arc::ptr_eq(&self.environment_owner, typed.environment_owner())
            && Arc::ptr_eq(&self.typed_analysis_owner, typed.analysis_owner())
    }

    /// 返回稳定源码顺序的所有权诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 返回稳定 symbol 顺序的参数 binding 能力。
    #[must_use]
    pub fn bindings(&self) -> &[OwnershipBindingDescriptor] {
        &self.bindings
    }

    /// 查询参数 binding 能力。
    #[must_use]
    pub fn binding_kind(&self, symbol: SymbolId) -> Option<OwnershipBindingKind> {
        self.bindings
            .binary_search_by_key(&symbol.index(), |binding| binding.symbol().index())
            .ok()
            .map(|index| self.bindings[index].kind())
    }

    /// 返回源码顺序的有效调用期 loans。
    #[must_use]
    pub fn loans(&self) -> &[LoanFact] {
        &self.loans
    }

    /// 查询由指定 argument 建立的 loan。
    #[must_use]
    pub fn loan_begin(&self, argument: ExpressionId) -> Option<&LoanFact> {
        self.loans.iter().find(|loan| loan.argument() == argument)
    }

    /// 返回在指定同步 call 返回时结束的 loans。
    pub fn loans_ending_at(&self, call: ExpressionId) -> impl Iterator<Item = &LoanFact> {
        self.loans.iter().filter(move |loan| {
            loan.call() == call
                && self.loan_ends.iter().any(|end| {
                    end.call == call
                        && end.argument == loan.argument()
                        && end.point == LoanEndPoint::CallReturn(call)
                })
        })
    }

    /// 返回源码 / 控制流顺序的有效 drop facts。
    #[must_use]
    pub fn drops(&self) -> &[DropFact] {
        &self.drops
    }

    /// 返回运行时求值顺序稳定的 construction ownership plans。
    #[must_use]
    pub fn construction_plans(&self) -> &[ConstructionOwnershipPlan] {
        &self.construction_plans
    }

    /// 查询指定 typed construction 的 ordered-delivery/root plan。
    #[must_use]
    pub fn construction_plan(
        &self,
        expression: ExpressionId,
    ) -> Option<&ConstructionOwnershipPlan> {
        self.construction_plans
            .iter()
            .find(|plan| plan.construction() == expression)
    }

    /// 返回源码顺序稳定的 intrinsic `Rc<T>` ownership effects。
    #[must_use]
    pub fn rc_effects(&self) -> &[RcOwnershipEffect] {
        &self.rc_effects
    }

    /// 查询指定表达式的 intrinsic `Rc<T>` ownership effect。
    #[must_use]
    pub fn rc_effect(&self, expression: ExpressionId) -> Option<RcOwnershipEffect> {
        self.rc_effects
            .iter()
            .copied()
            .find(|effect| effect.expression() == expression)
    }

    /// 返回 lambda/source 顺序稳定的 capture facts。
    #[must_use]
    pub fn captures(&self) -> &[ClosureCaptureDescriptor] {
        &self.captures
    }

    /// 返回一个 lambda 的源码顺序 capture facts。
    pub fn captures_of(
        &self,
        lambda: ExpressionId,
    ) -> impl Iterator<Item = &ClosureCaptureDescriptor> {
        self.captures
            .iter()
            .filter(move |capture| capture.lambda() == lambda)
    }

    /// 查询具体 lambda environment 的能力事实。
    #[must_use]
    pub fn closure(&self, expression: ExpressionId) -> Option<ClosureDescriptor> {
        self.closures
            .iter()
            .copied()
            .find(|closure| closure.expression() == expression)
    }

    /// 查询一个规范化类型的结构化跨线程转移能力。
    #[must_use]
    pub fn transferability(&self, ty: TypeId) -> Option<Transferability> {
        self.transferabilities.get(ty.index()).copied()
    }

    /// 返回明确留给后续 Spec 的所有权事实。
    #[must_use]
    pub fn deferred(&self) -> &[OwnershipDeferredFact] {
        &self.deferred
    }
}

/// 一次实际执行路径上的 loan 终止边；先终止 loan，再执行该边的 drop facts。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoanEndPoint {
    /// 所有实参正常完成且同步调用已返回。
    CallReturn(ExpressionId),
    /// 尚未调用就由 return/break/continue 放弃调用求值。
    ControlTransfer(ExpressionId),
}

/// 保留原 loan 的 call/argument identity，不建立第二个 loan。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoanEndFact {
    pub(crate) call: ExpressionId,
    pub(crate) argument: ExpressionId,
    pub(crate) point: LoanEndPoint,
}
impl LoanEndFact {
    /// 返回原 loan 所属调用。
    pub fn call(&self) -> ExpressionId {
        self.call
    }
    /// 返回建立原 loan 的实参或 receiver。
    pub fn argument(&self) -> ExpressionId {
        self.argument
    }
    /// 返回实际终止边；abort 不产生 unwind 终止事实。
    pub fn point(&self) -> LoanEndPoint {
        self.point
    }
}
