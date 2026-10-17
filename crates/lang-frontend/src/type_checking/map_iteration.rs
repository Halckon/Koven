//! Map 的两具名借用分量；不虚构 element、Pair 或字段投影。
use super::{
    CompilationUnitTypes, IntrinsicTypeConstructor, TypeId, TypedFile, UnitExpressionId,
    UnitStatementId, UnitTypeId,
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::{SymbolId, UnitSymbolId},
};

/// 只由真实 intrinsic type identity 决定的 Map provider。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapIterationKind {
    /// 只读 Map。
    Map,
    /// 可变 Map；当前迭代仍只建立共享借用。
    MutableMap,
}
impl MapIterationKind {
    pub(crate) const fn from_constructor(value: IntrinsicTypeConstructor) -> Option<Self> {
        match value {
            IntrinsicTypeConstructor::Map => Some(Self::Map),
            IntrinsicTypeConstructor::MutableMap => Some(Self::MutableMap),
            _ => None,
        }
    }
    pub(crate) const fn constructor(self) -> IntrinsicTypeConstructor {
        match self {
            Self::Map => IntrinsicTypeConstructor::Map,
            Self::MutableMap => IntrinsicTypeConstructor::MutableMap,
        }
    }
}

/// 单次 source 求值及按 key/value 顺序的真实 binding；不授予 ownership/native 资格。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapIterationDescriptor<St = StatementId, E = ExpressionId, T = TypeId, S = SymbolId> {
    pub(crate) statement: St,
    pub(crate) source: E,
    pub(crate) source_type: T,
    pub(crate) kind: MapIterationKind,
    pub(crate) key_type: T,
    pub(crate) value_type: T,
    pub(crate) key_binding: S,
    pub(crate) value_binding: S,
}
impl<St: Copy, E: Copy, T: Copy, S: Copy> MapIterationDescriptor<St, E, T, S> {
    /// 真实 for 身份。
    pub const fn statement(self) -> St {
        self.statement
    }
    /// 包含透明 Group 在内的唯一 source 身份。
    pub const fn source(self) -> E {
        self.source
    }
    /// source 的精确 intrinsic 类型。
    pub const fn source_type(self) -> T {
        self.source_type
    }
    /// Map 或 MutableMap。
    pub const fn kind(self) -> MapIterationKind {
        self.kind
    }
    /// 原 K 类型。
    pub const fn key_type(self) -> T {
        self.key_type
    }
    /// 原 V 类型。
    pub const fn value_type(self) -> T {
        self.value_type
    }
    /// 第一个具名 binding。
    pub const fn key_binding(self) -> S {
        self.key_binding
    }
    /// 第二个具名 binding。
    pub const fn value_binding(self) -> S {
        self.value_binding
    }
    /// 源码声明顺序，不是字段列表。
    pub const fn bindings(self) -> [S; 2] {
        [self.key_binding, self.value_binding]
    }
}

/// compilation-unit 的同一封闭合同，所有节点与 symbol 带 source identity。
pub type UnitMapIterationDescriptor =
    MapIterationDescriptor<UnitStatementId, UnitExpressionId, UnitTypeId, UnitSymbolId>;

impl TypedFile {
    /// 已原子发布的 Map 迭代类型事实。
    pub fn map_iterations(&self) -> &[MapIterationDescriptor] {
        &self.map_descriptors.iterations
    }
    /// 按实际 for identity 查询。
    pub fn map_iteration(&self, statement: StatementId) -> Option<&MapIterationDescriptor> {
        self.map_iterations()
            .iter()
            .find(|plan| plan.statement() == statement)
    }
}
impl CompilationUnitTypes {
    /// source-qualified Map 迭代事实，按 statement 稳定排序。
    pub fn map_iterations(&self) -> &[UnitMapIterationDescriptor] {
        &self.map_descriptors.iterations
    }
    /// 按实际 source-qualified for identity 查询。
    pub fn map_iteration(&self, statement: UnitStatementId) -> Option<&UnitMapIterationDescriptor> {
        self.map_iterations()
            .iter()
            .find(|plan| plan.statement() == statement)
    }
}
