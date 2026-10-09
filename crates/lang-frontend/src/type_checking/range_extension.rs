//! 可信标准扩展的签名绑定；它不是实际根来源或可执行交付证明。
use super::{BorrowReturnOrigin, CallableResultSource, ParameterMode, RangeSourceKind};
use crate::source::{SourceId, Span};

/// 同一 typed 产物内的 canonical callable 与 compiler-bound receiver。
/// 构造权限只在 frontend 内部；single/unit 各保持自己的 ID 与类型表。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeExtensionBinding<C, T> {
    source: SourceId,
    callable: C,
    receiver_type: T,
    element_type: T,
    source_kind: RangeSourceKind,
    receiver_span: Span,
    from_span: Span,
    source_span: Span,
}

impl<C: Copy, T: Copy + Eq> RangeExtensionBinding<C, T> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bind(
        authorized: bool,
        source: SourceId,
        callable: C,
        receiver_type: T,
        receiver_shape: Option<(RangeSourceKind, T)>,
        result_element: Option<T>,
        mode: ParameterMode,
        contract: CallableResultSource,
        receiver_span: Span,
    ) -> Option<Self> {
        let CallableResultSource::Carrier(contract) = contract else {
            return None;
        };
        let (source_kind, element_type) = receiver_shape?;
        let from_span = contract.from_span()?;
        (authorized
            && mode == ParameterMode::Borrow
            && contract.origin() == BorrowReturnOrigin::Receiver
            && result_element == Some(element_type)
            && receiver_span.source_id() == source
            && from_span.source_id() == source
            && contract.source_span().source_id() == source)
            .then_some(Self {
                source,
                callable,
                receiver_type,
                element_type,
                source_kind,
                receiver_span,
                from_span,
                source_span: contract.source_span(),
            })
    }
    /// 返回授权时的实际源码身份。
    #[must_use]
    pub const fn source(self) -> SourceId {
        self.source
    }
    /// 返回同一分析中的 canonical callable。
    #[must_use]
    pub const fn callable(self) -> C {
        self.callable
    }
    /// 返回声明的 compiler-bound receiver 类型。
    #[must_use]
    pub const fn receiver_type(self) -> T {
        self.receiver_type
    }
    /// 返回接收者和 View 结果共享的元素类型模板。
    #[must_use]
    pub const fn element_type(self) -> T {
        self.element_type
    }
    /// 返回 List 或 View 来源形状。
    #[must_use]
    pub const fn source_kind(self) -> RangeSourceKind {
        self.source_kind
    }
    /// 返回本首片唯一允许的 Borrow 模式。
    #[must_use]
    pub const fn receiver_mode(self) -> ParameterMode {
        ParameterMode::Borrow
    }
    /// 返回声明合同中的 this 来源槽。
    #[must_use]
    pub const fn origin(self) -> BorrowReturnOrigin {
        BorrowReturnOrigin::Receiver
    }
    /// 返回 receiver 类型的可追溯源码区间。
    #[must_use]
    pub const fn receiver_span(self) -> Span {
        self.receiver_span
    }
    /// 返回 from 关键字区间。
    #[must_use]
    pub const fn from_span(self) -> Span {
        self.from_span
    }
    /// 返回 this 来源声明区间。
    #[must_use]
    pub const fn source_span(self) -> Span {
        self.source_span
    }
}
