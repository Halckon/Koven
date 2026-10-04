//! Unit for 的精确 provider 与 borrowed projection；不授予所有权或 native capability。
use super::{UnitExpressionId, UnitStatementId, UnitTypeId};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{ParameterMode, SequentialContainerKind},
};

/// 当前轮的借用 binding，所有 identity 均限定所属 source。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitSequentialIterationBinding {
    /// `_` 不声明 symbol。
    Discard,
    /// 借用当前 element。
    Name(UnitSymbolId),
    /// 按主构造器字段顺序的 borrowed projection，包含 discard。
    Destructure(Vec<UnitSequentialIterationComponent>),
}

impl UnitSequentialIterationBinding {
    /// 返回源码顺序的具名 binding；discard 不产生 symbol。
    pub fn symbols(&self) -> impl Iterator<Item = UnitSymbolId> + '_ {
        let name = match self {
            Self::Name(symbol) => Some(*symbol),
            _ => None,
        };
        let components = match self {
            Self::Destructure(parts) => parts.as_slice(),
            _ => &[],
        };
        name.into_iter()
            .chain(components.iter().filter_map(|part| part.symbol))
    }
}

/// 字段声明身份与本地 binding 分离；类型已替换实际泛型参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSequentialIterationComponent {
    pub(crate) field: UnitSymbolId,
    pub(crate) symbol: Option<UnitSymbolId>,
    pub(crate) ty: UnitTypeId,
}

impl UnitSequentialIterationComponent {
    /// 原始主构造器字段身份，可来自另一个 source。
    #[must_use]
    pub const fn field(&self) -> UnitSymbolId {
        self.field
    }
    /// 本轮 binding；discard 为 None。
    #[must_use]
    pub const fn symbol(&self) -> Option<UnitSymbolId> {
        self.symbol
    }
    /// 实例化后的分量类型。
    #[must_use]
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }
}

/// 仅由完整 Phase 2 检查发布，source 表达式只求值一次。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSequentialIterationDescriptor {
    pub(crate) statement: UnitStatementId,
    pub(crate) source: UnitExpressionId,
    pub(crate) source_type: UnitTypeId,
    pub(crate) provider: SequentialContainerKind,
    pub(crate) element_type: UnitTypeId,
    pub(crate) binding: UnitSequentialIterationBinding,
}

impl UnitSequentialIterationDescriptor {
    /// source-qualified for identity。
    #[must_use]
    pub const fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// 唯一 source 求值 identity。
    #[must_use]
    pub const fn source(&self) -> UnitExpressionId {
        self.source
    }
    /// intrinsic container 类型。
    #[must_use]
    pub const fn source_type(&self) -> UnitTypeId {
        self.source_type
    }
    /// compiler-bound provider 身份。
    #[must_use]
    pub const fn provider(&self) -> SequentialContainerKind {
        self.provider
    }
    /// 精确 element 类型。
    #[must_use]
    pub const fn element_type(&self) -> UnitTypeId {
        self.element_type
    }
    /// 本轮 borrowed binding 与投影。
    #[must_use]
    pub const fn binding(&self) -> &UnitSequentialIterationBinding {
        &self.binding
    }
    /// 迭代只授予共享借用，普通值复制另由 Copyable 规则决定。
    #[must_use]
    pub const fn delivery(&self) -> ParameterMode {
        ParameterMode::Borrow
    }
}
