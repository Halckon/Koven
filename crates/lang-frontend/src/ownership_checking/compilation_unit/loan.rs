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
}
