//! 普通借用的实际来源、结果绑定依赖与显式终止事实。

use crate::{
    ast::ExpressionId,
    parser::{Expression, NameMarker, ParsedFile},
    source::Span,
};

use super::OwnershipCheckingError;

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

/// 已证明交付新内联 descriptor 的实际根；与既有存储借用返回分离。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeReturnOriginFact<E, T> {
    expression: E,
    origin: T,
    declaration_span: Span,
}
impl<E: Copy, T> RangeReturnOriginFact<E, T> {
    pub(crate) const fn new(expression: E, origin: T, declaration_span: Span) -> Self {
        Self {
            expression,
            origin,
            declaration_span,
        }
    }
    /// 实际返回构造/已证明转发的表达式。
    pub const fn expression(&self) -> E {
        self.expression
    }
    /// 经过实际 source 操作数验证的根。
    pub const fn origin(&self) -> &T {
        &self.origin
    }
    /// 真实 producer from 标记。
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ReturnSource<S> {
    pub(crate) symbol: Option<S>,
    pub(crate) span: Span,
    pub(crate) marker: Span,
    pub(crate) new_range: bool,
}

/// 新 descriptor 的短期使用边界，不延长普通 borrow val 的来源寿命。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeUseSite<E> {
    /// metadata 与根依赖保持到同步 Borrow 调用结束。
    Call(E),
    /// metadata 与 hidden root 由既有 provider 正常退出计划结束。
    Iteration,
}

/// 真实构造调用的 source loan 被短期 descriptor/provider 延续。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeUseFact<E, T> {
    pub(crate) expression: E,
    pub(crate) source_loan: BorrowSourceLoan<E>,
    pub(crate) origin: T,
    pub(crate) site: RangeUseSite<E>,
}
impl<E: Copy, T> RangeUseFact<E, T> {
    /// 实际求值的新 descriptor 调用。
    pub const fn expression(&self) -> E {
        self.expression
    }
    /// 构造调用实际建立并续接的来源 loan。
    pub const fn source_loan(&self) -> BorrowSourceLoan<E> {
        self.source_loan
    }
    /// 从实际来源操作数验证的根 owner/place。
    pub const fn origin(&self) -> &T {
        &self.origin
    }
    /// 短期使用结束边界，不能提升为持久绑定。
    pub const fn site(&self) -> RangeUseSite<E> {
        self.site
    }
}

/// Group 透明；普通包装调用只沿 Phase 2 发布的唯一实际来源实参追踪。
pub(crate) fn origin_expression(
    parsed: &ParsedFile,
    mut expression: ExpressionId,
    call_source: impl Fn(ExpressionId) -> Option<ExpressionId>,
) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
    loop {
        match parsed.ast().expressions().get(expression)?.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Call { .. } => {
                let Some(source) = call_source(expression) else {
                    return Ok(None);
                };
                expression = source;
            }
            Expression::Name | Expression::This | Expression::Member { .. } => {
                return Ok(Some(expression));
            }
            _ => return Ok(None),
        }
    }
}

impl super::OwnershipCheckedFile {
    /// 实际返回来源证明；不将它当作 caller loan 的结束/恢复计划。
    #[must_use]
    pub fn borrow_return_origins(
        &self,
    ) -> &[BorrowReturnOriginFact<ExpressionId, super::LoanTarget>] {
        &self.borrow_return_origins
    }
}

impl super::CompilationUnitOwnership {
    /// source-qualified 的实际返回来源证明。
    #[must_use]
    pub fn borrow_return_origins(
        &self,
    ) -> &[BorrowReturnOriginFact<crate::type_checking::UnitExpressionId, super::UnitLoanTarget>]
    {
        &self.borrow_return_origins
    }
}

pub(crate) const fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
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

/// borrow val 的实际存储种类；新 metadata 不拥有根或元素。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowBindingStorage {
    /// 借用既有普通值存储。
    BorrowedStorage,
    /// 借用既有 carrier metadata，保留父 metadata 依赖。
    BorrowedCarrierMetadata,
    /// 新交付的 root-flat 内联描述符存储。
    NewRangeDescriptor,
}

/// 确定结果绑定的存储种类、真实来源 lease、来源 loan 与必要父依赖。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorrowBindingFact<E, S, T> {
    pub(crate) binding: S,
    pub(crate) initializer: E,
    pub(crate) storage: BorrowBindingStorage,
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
    /// 返回既有存储借用或新描述符交付的明确事实。
    pub const fn storage(&self) -> BorrowBindingStorage {
        self.storage
    }
    /// 经过别名和投影解析的来源 lease；新 metadata 由 storage() 区分。
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
    pub(crate) range_returns: Vec<RangeReturnOriginFact<E, T>>,
    pub(crate) range_uses: Vec<RangeUseFact<E, T>>,
}
impl<E, S, T, P> Default for BorrowResultFacts<E, S, T, P> {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            ends: Vec::new(),
            forwarded: Vec::new(),
            range_returns: Vec::new(),
            range_uses: Vec::new(),
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
    /// 新 descriptor 的实际返回根证明；不使用普通借用返回 ABI。
    pub fn range_return_origins(&self) -> &[RangeReturnOriginFact<E, T>] {
        &self.range_returns
    }
    /// 返回实际交付给 caller 的来源 loan，不能在 CallReturn 结束。
    pub fn forwarded_source_loans(&self) -> &[BorrowSourceLoan<E>] {
        &self.forwarded
    }
    /// 同步调用/for 的短期新 descriptor 及真实根来源。
    pub fn range_uses(&self) -> &[RangeUseFact<E, T>] {
        &self.range_uses
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
