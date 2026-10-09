//! 可复用范围构造的实际操作数；借用来源证明由 ownership 阶段独立发布。
use crate::{name_resolution::ExternalSymbolId, source::Span};

/// 构造原语能读取的 compiler-bound 来源种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeSourceKind {
    /// 来源是持有元素的 List。
    List,
    /// 来源是继承根 loan 的 View，边界相对该 View。
    View,
}

/// 来自已授权来源且完成类型检查的范围构造；不包含算法名或推测的根。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeConstructionDescriptor<E, T> {
    pub(crate) expression: E,
    pub(crate) intrinsic: ExternalSymbolId,
    pub(crate) source: E,
    pub(crate) begin: E,
    pub(crate) end: E,
    pub(crate) source_kind: RangeSourceKind,
    pub(crate) element_type: T,
    pub(crate) result_type: T,
    pub(crate) source_span: Span,
}

impl<E: Copy, T: Copy> RangeConstructionDescriptor<E, T> {
    /// 构造表达式 identity。
    pub const fn expression(&self) -> E {
        self.expression
    }
    /// 已绑定的原语 callable identity。
    pub const fn intrinsic(&self) -> ExternalSymbolId {
        self.intrinsic
    }
    /// 实际来源操作数，不能由算法名称推断。
    pub const fn source(&self) -> E {
        self.source
    }
    /// 相对来源的半开范围起点操作数。
    pub const fn begin(&self) -> E {
        self.begin
    }
    /// 相对来源的半开范围终点操作数。
    pub const fn end(&self) -> E {
        self.end
    }
    /// compiler-bound 来源种类。
    pub const fn source_kind(&self) -> RangeSourceKind {
        self.source_kind
    }
    /// 规范化元素 identity。
    pub const fn element_type(&self) -> T {
        self.element_type
    }
    /// 新内联 View 的规范化类型。
    pub const fn result_type(&self) -> T {
        self.result_type
    }
    /// 来源操作数的实际 Span。
    pub const fn source_span(&self) -> Span {
        self.source_span
    }
}

/// compiler-bound View 的只读 size；不把它归入 owning sequential container。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeSizeDescriptor<E, T> {
    pub(crate) expression: E,
    pub(crate) receiver: E,
    pub(crate) receiver_type: T,
    pub(crate) result_type: T,
    pub(crate) span: Span,
}
impl<E: Copy, T: Copy> RangeSizeDescriptor<E, T> {
    /// member 表达式 identity。
    pub const fn expression(&self) -> E {
        self.expression
    }
    /// 实际 metadata receiver。
    pub const fn receiver(&self) -> E {
        self.receiver
    }
    /// compiler-bound View 类型。
    pub const fn receiver_type(&self) -> T {
        self.receiver_type
    }
    /// Int size 类型。
    pub const fn result_type(&self) -> T {
        self.result_type
    }
    /// 完整 member Span。
    pub const fn span(&self) -> Span {
        self.span
    }
}
impl super::TypedFile {
    /// 本轮真实 View size facts。
    pub fn range_sizes(&self) -> &[RangeSizeDescriptor<crate::ast::ExpressionId, super::TypeId>] {
        &self.range_sizes
    }
    /// 查询一个已验证的 View size。
    pub fn range_size(
        &self,
        expression: crate::ast::ExpressionId,
    ) -> Option<&RangeSizeDescriptor<crate::ast::ExpressionId, super::TypeId>> {
        self.range_sizes
            .iter()
            .find(|fact| fact.expression == expression)
    }
}
impl super::CompilationUnitTypes {
    /// source-qualified View size facts。
    pub fn range_sizes(
        &self,
    ) -> &[RangeSizeDescriptor<super::UnitExpressionId, super::UnitTypeId>] {
        &self.range_sizes
    }
    /// 查询一个 source-qualified View size。
    pub fn range_size(
        &self,
        expression: super::UnitExpressionId,
    ) -> Option<&RangeSizeDescriptor<super::UnitExpressionId, super::UnitTypeId>> {
        self.range_sizes
            .iter()
            .find(|fact| fact.expression == expression)
    }
}
