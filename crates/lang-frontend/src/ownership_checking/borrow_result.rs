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

/// 被结果延续的原调用 loan identity；不把来源重新借成无关的 loan。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowSourceLoan<E> {
    pub(crate) call: E,
    pub(crate) argument: E,
}
impl<E: Copy> BorrowSourceLoan<E> {
    /// 建立来源 loan 的调用。
    pub const fn call(&self) -> E {
        self.call
    }
    /// 建立来源 loan 的实参。
    pub const fn argument(&self) -> E {
        self.argument
    }
}

/// 确定 shared 结果的绑定 identity、真实来源及父依赖。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorrowBindingFact<E, S, T> {
    pub(crate) binding: S,
    pub(crate) initializer: E,
    pub(crate) origin: T,
    pub(crate) parent: Option<S>,
    pub(crate) source_loan: Option<BorrowSourceLoan<E>>,
    pub(crate) marker: Span,
}
impl<E: Copy, S: Copy, T> BorrowBindingFact<E, S, T> {
    /// 结果 loan 的唯一绑定 identity。
    pub const fn binding(&self) -> S {
        self.binding
    }
    /// 完成初始化的源码表达式。
    pub const fn initializer(&self) -> E {
        self.initializer
    }
    /// 经过别名和投影解析的实际存储。
    pub const fn origin(&self) -> &T {
        &self.origin
    }
    /// shared 重借用依赖的父结果绑定。
    pub const fn parent(&self) -> Option<S> {
        self.parent
    }
    /// 调用返回时交接给结果的来源 loan；stable-place 绑定没有调用 loan。
    pub const fn source_loan(&self) -> Option<BorrowSourceLoan<E>> {
        self.source_loan
    }
    /// 显式 borrow val 的建立位置。
    pub const fn marker_span(&self) -> Span {
        self.marker
    }
}

/// 实际可达边上的结果结束；子结果先结束，然后解除其来源/父依赖，最后清理 owner。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowBindingEndFact<S, P> {
    pub(crate) binding: S,
    pub(crate) point: P,
}
impl<S: Copy, P: Copy> BorrowBindingEndFact<S, P> {
    /// 被结束的结果 loan identity。
    pub const fn binding(&self) -> S {
        self.binding
    }
    /// 终止边；同点的 owner drop 必须在这些结果终止之后。
    pub const fn point(&self) -> P {
        self.point
    }
}

/// 只读、原子发布的 caller continuation 与转发合同。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorrowResultFacts<E, S, T, P> {
    pub(crate) bindings: Vec<BorrowBindingFact<E, S, T>>,
    pub(crate) ends: Vec<BorrowBindingEndFact<S, P>>,
    pub(crate) forwarded: Vec<BorrowSourceLoan<E>>,
}
impl<E, S, T, P> Default for BorrowResultFacts<E, S, T, P> {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            ends: Vec::new(),
            forwarded: Vec::new(),
        }
    }
}
impl<E, S, T, P> BorrowResultFacts<E, S, T, P> {
    /// 源码顺序的 shared 结果绑定。
    pub fn bindings(&self) -> &[BorrowBindingFact<E, S, T>] {
        &self.bindings
    }
    /// 实际终止边，保留子结果先于父结果的顺序。
    pub fn ends(&self) -> &[BorrowBindingEndFact<S, P>] {
        &self.ends
    }
    /// 普通返回实际交付给 caller 的来源 loan，不能在 CallReturn 结束。
    pub fn forwarded_source_loans(&self) -> &[BorrowSourceLoan<E>] {
        &self.forwarded
    }
}

pub(crate) type FileBorrowResults = BorrowResultFacts<
    ExpressionId,
    crate::name_resolution::SymbolId,
    super::LoanTarget,
    super::DropPoint,
>;
pub(crate) type UnitBorrowResults = BorrowResultFacts<
    crate::type_checking::UnitExpressionId,
    crate::name_resolution::UnitSymbolId,
    super::UnitLoanTarget,
    super::UnitDropPoint,
>;

impl super::OwnershipCheckedFile {
    /// 显式结果绑定及其来源延续、父依赖和终止边。
    pub fn borrow_results(&self) -> &FileBorrowResults {
        &self.borrow_results
    }
}
impl super::CompilationUnitOwnership {
    /// source-qualified 的结果 continuation 与终止边。
    pub fn borrow_results(&self) -> &UnitBorrowResults {
        &self.borrow_results
    }
}
