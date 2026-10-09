//! 顺序容器 for 的借用交付事实；不证明 Phase 3 loan 或 native 可执行性。
use super::{ParameterMode, SequentialContainerKind, TypeId};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
};

/// 编译器绑定的只读迭代来源；View 不取得元素所有权。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationProvider {
    /// 拥有元素的顺序容器。
    Sequential(SequentialContainerKind),
    /// 借用区间描述符。
    RangeView,
}

/// 本轮 element access 的 binding；所有名称均为 loop-scoped Borrow。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequentialIterationBinding {
    /// 单 `_`，没有源码 symbol。
    Discard,
    /// 单个 element binding。
    Name(SymbolId),
    /// 主构造器字段顺序，包含丢弃分量。
    Destructure(Vec<SequentialIterationComponent>),
}

/// 一个 value-class borrowed field projection。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequentialIterationComponent {
    pub(crate) field: SymbolId,
    pub(crate) symbol: Option<SymbolId>,
    pub(crate) ty: TypeId,
}
impl SequentialIterationComponent {
    /// 主构造器字段身份。
    #[must_use]
    pub fn field(&self) -> SymbolId {
        self.field
    }
    /// 循环 binding；丢弃为 None。
    #[must_use]
    pub fn symbol(&self) -> Option<SymbolId> {
        self.symbol
    }
    /// 替换实际泛型参数后的字段类型。
    #[must_use]
    pub fn ty(&self) -> TypeId {
        self.ty
    }
}

/// 一个 source 只求值一次的 compiler-bound provider 计划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequentialIterationDescriptor {
    pub(crate) statement: StatementId,
    pub(crate) source: ExpressionId,
    pub(crate) source_type: TypeId,
    pub(crate) provider: IterationProvider,
    pub(crate) element_type: TypeId,
    pub(crate) binding: SequentialIterationBinding,
}
impl SequentialIterationDescriptor {
    /// 稳定 for identity。
    #[must_use]
    pub fn statement(&self) -> StatementId {
        self.statement
    }
    /// 唯一 source 求值 identity。
    #[must_use]
    pub fn source(&self) -> ExpressionId {
        self.source
    }
    /// source 的 intrinsic container 类型。
    #[must_use]
    pub fn source_type(&self) -> TypeId {
        self.source_type
    }
    /// 编译器绑定的 provider 身份。
    #[must_use]
    pub fn provider(&self) -> IterationProvider {
        self.provider
    }
    /// 每轮 element 的精确类型。
    #[must_use]
    pub fn element_type(&self) -> TypeId {
        self.element_type
    }
    /// 本轮 borrowed binding。
    #[must_use]
    pub fn binding(&self) -> &SequentialIterationBinding {
        &self.binding
    }
    /// 只授予 shared delivery，不推导 owned copy 或 move。
    #[must_use]
    pub fn delivery(&self) -> ParameterMode {
        ParameterMode::Borrow
    }
}
