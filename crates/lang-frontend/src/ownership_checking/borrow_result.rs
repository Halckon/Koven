use crate::source::Span;

/// 从实际返回表达式验证的来源；E/T 保持 single 与 source-qualified unit 的 ID 边界。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorrowReturnOriginFact<E, T> {
    expression: E,
    origin: T,
    declaration_span: Span,
}

impl<E: Copy, T> BorrowReturnOriginFact<E, T> {
    /// 返回实际交付的表达式 identity。
    #[must_use]
    pub const fn expression(&self) -> E {
        self.expression
    }

    /// 返回已验证的稳定 place/receiver 来源。
    #[must_use]
    pub const fn origin(&self) -> &T {
        &self.origin
    }

    /// 返回声明端 borrow marker，供下游未支持诊断定位。
    #[must_use]
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }
}
