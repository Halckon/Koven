use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::SymbolId,
    source::Span,
    type_checking::{NominalId, TypeId},
};

pub use super::checked_file::OwnershipCheckedFile;
pub(crate) use super::checked_file::OwnershipCheckedParts;

/// 可由 Phase 3 精确识别的源码 place。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnershipPlace {
    root: SymbolId,
    fields: Vec<SymbolId>,
    elements: Vec<ElementIndexIdentity>,
}

impl OwnershipPlace {
    pub(crate) fn new(root: SymbolId, fields: Vec<SymbolId>) -> Self {
        Self {
            root,
            fields,
            elements: Vec::new(),
        }
    }

    pub(crate) fn push_field(&mut self, field: SymbolId) -> bool {
        if !self.elements.is_empty() {
            return false;
        }
        self.fields.push(field);
        true
    }

    pub(crate) fn push_element(&mut self, element: ElementIndexIdentity) {
        self.elements.push(element);
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
    pub fn element(&self) -> Option<ElementIndexIdentity> {
        self.elements.last().copied()
    }

    /// 返回字段路径之后从外到内的完整逻辑索引路径。
    #[must_use]
    pub fn elements(&self) -> &[ElementIndexIdentity] {
        &self.elements
    }

    /// 返回该 place 是否精确表示根绑定自身。
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.fields.is_empty() && self.elements.is_empty()
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
        self.elements
            .iter()
            .zip(&other.elements)
            .all(|(&left, &right)| left.may_alias(right))
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
    /// 当前 instance callable 的唯一 this receiver，以 nominal identity 限定来源。
    This(NominalId),
}

/// 一次成功建立的 loan；终止或结果交接由 `loan_ends` 与 `borrow_results` 描述。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoanFact {
    call: ExpressionId,
    argument: ExpressionId,
    target: LoanTarget,
    kind: LoanKind,
    begin_span: Span,
    end_span: Span,
    receiver_reservation: bool,
    activation_point: Option<DropPoint>,
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
            receiver_reservation: false,
            activation_point: None,
        }
    }

    pub(crate) fn reserve_receiver(mut self) -> Self {
        self.receiver_reservation = true;
        self
    }

    pub(crate) fn activate_receiver(&mut self) {
        self.activation_point = Some(DropPoint::CallEntry(self.call));
    }

    /// 返回该 loan 是否先预留再在方法调用入口激活。
    #[must_use]
    pub const fn is_receiver_reservation(&self) -> bool {
        self.receiver_reservation
    }

    /// 返回可达的 receiver 激活点；实参提前转移或 abort 时为 None。
    #[must_use]
    pub const fn activation_point(&self) -> Option<DropPoint> {
        self.activation_point
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

    /// 返回同步调用的结束/交接位置；结果来源的终止另见 `borrow_results`。
    #[must_use]
    pub const fn end_span(&self) -> Span {
        self.end_span
    }
}

/// Phase 4 可消费的析构边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPoint {
    /// Lambda 调用入口的未使用 owned 参数。
    LambdaEntry(ExpressionId),
    /// 最后一次读取或 replacement RHS 完成后。
    AfterExpression(ExpressionId),
    /// 非消费式 binary 已读完全部 operand、但结果仍存活时。
    AfterBinaryOperands(ExpressionId),
    /// 未使用 binding 建立或完整 statement 完成后。
    AfterStatement(StatementId),
    /// Callee 与全部实参求值完成后、实际调用之前。
    CallEntry(ExpressionId),
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
    /// 先前显式 Move/清理已清空的槽不再析构；同一清理点重复消费槽仍是无效事实。
    Captured {
        /// 具体环境值；lambda/source 仅描述其中的捕获槽。
        owner: super::CleanupOwnerValueId,
        /// 拥有 environment 的 lambda。
        closure: ExpressionId,
        /// 被析构的捕获来源。
        source: ClosureCaptureSource,
        /// 形成此槽的实际来源，不能在清理时重新读取 source binding。
        value: super::CleanupCaptureValue,
    },
    /// 已离开词法 binding、仍由 borrowed closure 保护的 source owner。
    /// `DropFact::owner` 指向该环境的 source-owner 关系槽。
    RetainedSource(ClosureCaptureSource),
}

/// 一个确定的 ASAP 析构事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropFact {
    owner: Option<super::CleanupOwnerValueId>,
    capture_slot: Option<super::CleanupCaptureSlotId>,
    instance_address: Option<super::CleanupInstanceAddressId>,
    condition: Option<super::CleanupConditionId>,
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
            owner: None,
            capture_slot: None,
            instance_address: None,
            condition: None,
            point,
            target,
            value_origin,
        }
    }

    /// 该边界内额外需要满足的保存路径条件；None 表示无条件。
    #[must_use]
    pub const fn condition(self) -> Option<super::CleanupConditionId> {
        self.condition
    }

    /// Named、已运输 pending temporary、迭代 source temporary 或直接 owned capture 的
    /// 源值定义身份。target 仍定位访问位置；其它 temporary、element 与紧邻环境槽由 target 描述。
    #[must_use]
    pub const fn owner(self) -> Option<super::CleanupOwnerValueId> {
        self.owner
    }

    /// 已形成 closure 的析构槽；消费时须查询该环境当次实例的槽占用状态。
    /// 此 ID 仅是静态布局位置；递归/多路径 deferred 尚缺父实例边定位。
    /// `None` 表示该事实不是已形成环境的槽，或 phi 实例布局尚未提供此关系。
    #[must_use]
    pub const fn capture_slot(self) -> Option<super::CleanupCaptureSlotId> {
        self.capture_slot
    }

    /// Captured/retained-source drop 所属环境的动态根与捕获路径；
    /// 被释放的值由 `capture_slot()` 指定。
    /// `None` 表示此事实尚不可沿已保存环境边定位。
    #[must_use]
    pub const fn instance_address(self) -> Option<super::CleanupInstanceAddressId> {
        self.instance_address
    }

    pub(crate) const fn with_owner(mut self, owner: super::CleanupOwnerValueId) -> Self {
        self.owner = Some(owner);
        self
    }

    pub(crate) const fn with_capture_slot(mut self, slot: super::CleanupCaptureSlotId) -> Self {
        self.capture_slot = Some(slot);
        self
    }

    pub(crate) const fn with_instance_address(
        mut self,
        address: super::CleanupInstanceAddressId,
    ) -> Self {
        self.instance_address = Some(address);
        self
    }

    pub(crate) const fn with_condition(mut self, condition: super::CleanupConditionId) -> Self {
        self.condition = Some(condition);
        self
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
    /// String when 的比较存活期尚未具有完整的稳定 subject/候选清理证明。
    StringWhenComparisonLifetime,
    /// 资源 owner 的条件或循环实例尚无可执行的词法清理运输。
    ResourceLifetime,
    /// 原子 root 置换尚不能运输 closure 或包含 closure 的来源图。
    OwnershipPrimitiveClosureTransport,
    /// 顺序容器 index place 等待 SPEC-0030。
    IndexPlace,
    /// 未具有静态参数契约的 instance member receiver。
    MemberReceiver,
    /// 循环携带的 closure 递归捕获旧环境，有限 phi 树无法完整运输。
    RecursiveClosureCapture,
    /// 循环 phi 尚不能运输紧邻 callable 环境中的捕获实例。
    EnclosingEnvironmentCapture,
    /// 不同捕获路径的环境实例可能并存，phi 槽尚缺实例限定或互斥证明。
    AmbiguousClosureInstanceTransport,
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
