use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::Diagnostic,
    name_resolution::SymbolId,
    source::{SourceId, Span},
};

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

/// 一次成功建立并在同步调用返回时结束的 loan。
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
}

/// 一个确定的 ASAP 析构事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropFact {
    point: DropPoint,
    target: DropTarget,
    value_origin: Span,
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
    /// lambda capture 等待 SPEC-0032。
    LambdaCapture,
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
    source_id: SourceId,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<OwnershipBindingDescriptor>,
    loans: Vec<LoanFact>,
    drops: Vec<DropFact>,
    deferred: Vec<OwnershipDeferredFact>,
}

impl OwnershipCheckedFile {
    pub(crate) const fn new(
        source_id: SourceId,
        diagnostics: Vec<Diagnostic>,
        bindings: Vec<OwnershipBindingDescriptor>,
        loans: Vec<LoanFact>,
        drops: Vec<DropFact>,
        deferred: Vec<OwnershipDeferredFact>,
    ) -> Self {
        Self {
            source_id,
            diagnostics,
            bindings,
            loans,
            drops,
            deferred,
        }
    }

    /// 返回输入源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
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
        self.loans.iter().filter(move |loan| loan.call() == call)
    }

    /// 返回源码 / 控制流顺序的有效 drop facts。
    #[must_use]
    pub fn drops(&self) -> &[DropFact] {
        &self.drops
    }

    /// 返回明确留给后续 Spec 的所有权事实。
    #[must_use]
    pub fn deferred(&self) -> &[OwnershipDeferredFact] {
        &self.deferred
    }
}
