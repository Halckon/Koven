use super::OwnershipCheckingError;
use crate::{
    ast::ExpressionId,
    parser::{Expression, NameMarker, ParsedFile},
    source::Span,
};

/// 从实际返回表达式验证的来源；E/T 保持 single 与 source-qualified unit 的 ID 边界。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorrowReturnOriginFact<E, T> {
    expression: E,
    origin: T,
    declaration_span: Span,
}

impl<E: Copy, T> BorrowReturnOriginFact<E, T> {
    pub(crate) const fn new(expression: E, origin: T, declaration_span: Span) -> Self {
        Self {
            expression,
            origin,
            declaration_span,
        }
    }

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

#[derive(Clone, Copy)]
pub(crate) struct ReturnSource<S> {
    pub(crate) symbol: Option<S>,
    pub(crate) span: Span,
    pub(crate) marker: Span,
}

/// Only a stable place can prove this producer's origin; calls need caller continuation.
pub(crate) fn origin_expression(
    parsed: &ParsedFile,
    mut expression: ExpressionId,
) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
    loop {
        match parsed.ast().expressions().get(expression)?.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Name | Expression::Member { .. } => return Ok(Some(expression)),
            _ => return Ok(None),
        }
    }
}

impl super::OwnershipCheckedFile {
    /// Actual return-place proofs; these do not authorize caller loan continuation.
    #[must_use]
    pub fn borrow_return_origins(
        &self,
    ) -> &[BorrowReturnOriginFact<ExpressionId, super::LoanTarget>] {
        &self.borrow_return_origins
    }
}

pub(crate) const fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

impl super::CompilationUnitOwnership {
    /// Actual source-qualified return proofs; these do not authorize caller continuation.
    #[must_use]
    pub fn borrow_return_origins(
        &self,
    ) -> &[BorrowReturnOriginFact<crate::type_checking::UnitExpressionId, super::UnitLoanTarget>]
    {
        &self.borrow_return_origins
    }
}
