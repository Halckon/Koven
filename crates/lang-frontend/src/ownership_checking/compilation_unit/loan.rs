//! source-qualified 同步 loan 的目标与事实合同。
use super::*;

/// unit loan 的稳定目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitLoanTarget {
    /// 名称、字段或 terminal element place。
    Place(UnitOwnershipPlace),
    /// 当前 callable 的稳定 receiver identity。
    This(DeclarationId),
    /// 延长到同步调用返回的 temporary。
    Temporary(UnitExpressionId),
}

/// 一次成功建立的 source-qualified 同步 loan 描述符。
/// 借用结果可延续其来源 loan；此描述符本身不证明 loan 已在 call 返回时结束。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanFact {
    call: UnitExpressionId,
    argument: UnitExpressionId,
    pub(super) target: UnitLoanTarget,
    kind: LoanKind,
    begin_span: Span,
    end_span: Span,
    parameter_span: Option<Span>,
}

impl UnitLoanFact {
    pub(in crate::ownership_checking) const fn new(
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

    /// 返回所属同步 call 的源码结束位置；不是实际 loan-end 事件的位置。
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

/// 普通直接 Borrow 调用实参的实际 planner 结束事件；不是完整 Unit loan-end 流。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDirectCallLoanEndFact {
    pub(super) loan: UnitLoanFact,
    pub(super) point: UnitDropPoint,
    pub(super) preceding_drops: usize,
    pub(super) preceding_call_loan_ends: usize,
}

impl UnitDirectCallLoanEndFact {
    /// 返回原始 source-qualified loan 描述符，保留 call、argument 和 target 身份。
    #[must_use]
    pub const fn loan(&self) -> &UnitLoanFact {
        &self.loan
    }

    /// 实际返回或放弃该调用前缀的控制转移点。
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        self.point
    }

    /// 同 point 普通 drops 中先于此事件的数量；事件间保持发布顺序。
    /// 这不是 receiver 条件析构或 capture/provider 动作的统一执行索引。
    #[must_use]
    pub const fn preceding_drops(&self) -> usize {
        self.preceding_drops
    }
    /// 同一分析、同 source-qualified point 中先于此事件的实际 EndCallLoan 动作数。
    /// 可跨 direct/prefix 切片比较同点顺序；含未公开的 call-loan 动作，编号可能有空隙。
    /// 不含 receiver/result/drop 动作，不是全局执行索引或完整 loan-end 流。
    #[must_use]
    pub const fn preceding_call_loan_ends(&self) -> usize {
        self.preceding_call_loan_ends
    }
}

/// Exclusive 根实参因 return/break/continue 放弃调用前缀的实际结束事件。
/// 仅为观测切片，不覆盖正常返回、投影、receiver 或完整 owner 图。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitExclusiveCallPrefixEndFact {
    pub(super) loan: UnitLoanFact,
    pub(super) point: UnitDropPoint,
    pub(super) preceding_drops: usize,
    pub(super) preceding_call_loan_ends: usize,
}

impl UnitExclusiveCallPrefixEndFact {
    /// 返回原始 source-qualified loan 描述符，保留 call、argument 和 target 身份。
    #[must_use]
    pub const fn loan(&self) -> &UnitLoanFact {
        &self.loan
    }

    /// 放弃调用前缀的实际控制转移点。
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        self.point
    }

    /// 同 point 普通 drops 中先于此事件的数量；事件间保持发布顺序。
    /// 这不是 receiver 条件析构或 capture/provider 动作的统一执行索引。
    #[must_use]
    pub const fn preceding_drops(&self) -> usize {
        self.preceding_drops
    }

    /// 同一分析、同 source-qualified 控制转移点中先行的实际 EndCallLoan 动作数。
    /// 与 direct/borrow-prefix 切片共用同点顺序，含未公开的动作，编号可能有空隙。
    /// 不含 receiver/result/drop 动作，不是全局执行索引或完整 loan-end 流。
    #[must_use]
    pub const fn preceding_call_loan_ends(&self) -> usize {
        self.preceding_call_loan_ends
    }
}

/// 具名 Shared 来源根的正常借用返回交接；不是完整 loan-end/cleanup 流。
/// 来自已完成调用的真实 pending tokens，不包含未完成调用或最终结果终止。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowCallHandoffFact {
    pub(super) auxiliary_loan_ends: Vec<UnitLoanFact>,
    pub(super) binding: crate::name_resolution::UnitSymbolId,
    pub(super) source_loan: UnitLoanFact,
}
impl UnitBorrowCallHandoffFact {
    /// 正常返回时结束的辅助 Shared owned 根或实际 pending 临时 owner tokens，保持原 pending 顺序。
    /// 排除来源 token；不覆盖借用绑定、provider、投影、父 owner 或全部 loan-end 流。
    /// 空列表不证明未覆盖的辅助 tokens 的状态，也不授予跨事件切片的统一顺序。
    #[must_use]
    pub fn auxiliary_loan_ends(&self) -> &[UnitLoanFact] {
        &self.auxiliary_loan_ends
    }

    /// 同一正常返回初始化的显式 borrow val；仅直接或透明 Group initializer、owned 根来源。
    #[must_use]
    pub const fn binding(&self) -> crate::name_resolution::UnitSymbolId {
        self.binding
    }

    /// 正常返回后由现有借用结果保护延续的唯一来源。
    #[must_use]
    pub const fn source_loan(&self) -> &UnitLoanFact {
        &self.source_loan
    }
    /// 实际完成该调用的返回点；不表示来源 loan 在此结束。
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        UnitDropPoint::CallReturn(self.source_loan.call)
    }
}

/// 实际进入借用 initializer 后被 return 放弃时，已求值来源 token 的结束切片。
/// 不要求结果 binding 已建立；不覆盖完整控制流或全部借用结束事件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowCallPrefixEndFact {
    pub(super) loan: UnitLoanFact,
    pub(super) point: UnitDropPoint,
    pub(super) preceding_drops: usize,
    pub(super) preceding_call_loan_ends: usize,
}

impl UnitBorrowCallPrefixEndFact {
    /// 返回原始 source-qualified loan 描述符，保留 call、argument 和 target 身份。
    #[must_use]
    pub const fn loan(&self) -> &UnitLoanFact {
        &self.loan
    }

    /// 实际返回或放弃该调用前缀的控制转移点。
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        self.point
    }

    /// 同 point 普通 drops 中先于此事件的数量；事件间保持发布顺序。
    /// 这不是 receiver 条件析构或 capture/provider 动作的统一执行索引。
    #[must_use]
    pub const fn preceding_drops(&self) -> usize {
        self.preceding_drops
    }
    /// 同一分析、同 source-qualified point 中先于此事件的实际 EndCallLoan 动作数。
    /// 可跨 direct/prefix 切片比较同点顺序；含未公开的 call-loan 动作，编号可能有空隙。
    /// 不含 receiver/result/drop 动作，不是全局执行索引或完整 loan-end 流。
    #[must_use]
    pub const fn preceding_call_loan_ends(&self) -> usize {
        self.preceding_call_loan_ends
    }
}

/// Actual source-token transport through a synchronous Borrow consumer.
/// This bounded normal-return transaction is not a complete loan-end stream.
/// It does not cover binding/forwarded sources, receivers, providers or unwind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowCallTransportFact {
    pub(super) source: UnitLoanFact,
    pub(super) consumer: UnitLoanFact,
}

impl UnitBorrowCallTransportFact {
    /// The inner call completed here, moving its actual source token into transport.
    /// This is a handoff, not a source end.
    #[must_use]
    pub const fn handoff_point(&self) -> UnitDropPoint {
        UnitDropPoint::CallReturn(self.source.call)
    }

    /// The actual consumer completed here and retired both transported tokens.
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        UnitDropPoint::CallReturn(self.consumer.call)
    }

    /// Consumer token ends before its source ancestor, in this transaction only.
    /// Other transactions and cleanup actions have no shared ordering through this API.
    #[must_use]
    pub const fn ended_loans(&self) -> [&UnitLoanFact; 2] {
        [&self.consumer, &self.source]
    }
}

/// Actual source token carried from a direct/grouped initializer to local binding termination.
/// This bounded transaction is not a complete loan-end stream or a shared cleanup order.
/// Parent/provider/projected sources and forwarded results are not covered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowBindingTransportFact {
    pub(super) source: UnitLoanFact,
    pub(super) binding: crate::name_resolution::UnitSymbolId,
    pub(super) point: UnitDropPoint,
}

impl UnitBorrowBindingTransportFact {
    /// The exact pending source token moved at initializer return, then consumed at termination.
    #[must_use]
    pub const fn source_loan(&self) -> &UnitLoanFact {
        &self.source
    }

    /// The successfully initialized local result whose actual retirement released the token.
    #[must_use]
    pub const fn binding(&self) -> crate::name_resolution::UnitSymbolId {
        self.binding
    }

    /// Actual initializer return; this handoff is not a source end.
    #[must_use]
    pub const fn handoff_point(&self) -> UnitDropPoint {
        UnitDropPoint::CallReturn(self.source.call)
    }

    /// Actual binding termination followed by source-token retirement, before owner cleanup.
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        self.point
    }
}

/// Actual Shared parameter-source token handed from an inner call to a wrapper return.
/// This bounded forwarding transaction does not end the source, map it to a caller
/// instance, or prove absence in a complete loan-end stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowForwardTransportFact {
    pub(super) source: UnitLoanFact,
    pub(super) returned: UnitExpressionId,
    pub(super) point: UnitDropPoint,
}

impl UnitBorrowForwardTransportFact {
    /// Original pending source token, preserved without reconstructing a loan descriptor.
    #[must_use]
    pub const fn source_loan(&self) -> &UnitLoanFact {
        &self.source
    }

    /// The complete returned expression, retaining any Group identity.
    #[must_use]
    pub const fn returned_expression(&self) -> UnitExpressionId {
        self.returned
    }

    /// Normal inner return where the source token moved into forwarding state.
    #[must_use]
    pub const fn handoff_point(&self) -> UnitDropPoint {
        UnitDropPoint::CallReturn(self.source.call)
    }

    /// Actual wrapper return where its result takes over the live source token.
    /// This is a transfer, not a source end or owner drop.
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        self.point
    }
}
