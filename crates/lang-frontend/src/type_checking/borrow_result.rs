use crate::source::Span;

/// 普通借用结果的唯一签名来源；不构造一等借用类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowReturnOrigin {
    /// 源码声明顺序的非 owning 参数。
    Parameter(usize),
    /// 当前实例 callable 的非 owning receiver。
    Receiver,
}

/// Phase 2 验证的普通借用结果合同；实际返回 origin 仍由 Phase 3 检查。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowReturnContract {
    origin: BorrowReturnOrigin,
    marker_span: Span,
    source_span: Span,
}

impl BorrowReturnContract {
    /// 返回唯一参数/receiver 来源。
    pub const fn origin(self) -> BorrowReturnOrigin {
        self.origin
    }
    /// 返回声明的真实 borrow marker。
    pub const fn marker_span(self) -> Span {
        self.marker_span
    }
    /// 返回声明的来源范围。
    pub const fn source_span(self) -> Span {
        self.source_span
    }
}
