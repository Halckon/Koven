use crate::ast::ExpressionId;

use super::{ParameterMode, TypeId, TypedFile};

/// 编译器预声明、不能由源码同名函数冒充的顺序容器列表式构造。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntrinsicCallable {
    /// `arrayOf(...)`.
    ArrayOf,
    /// `listOf(...)`.
    ListOf,
    /// `mutableListOf(...)`.
    MutableListOf,
}

/// v1 封闭的顺序容器种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SequentialContainerKind {
    /// 固定长度、元素可替换。
    Array,
    /// 只读元素与长度。
    List,
    /// 长度与元素都可变。
    MutableList,
}

impl SequentialContainerKind {
    /// 返回 element place 是否允许替换或独占借用。
    #[must_use]
    pub const fn elements_are_mutable(self) -> bool {
        matches!(self, Self::Array | Self::MutableList)
    }
}

/// 核心顺序容器构造的封闭形状。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerConstructionKind {
    /// `arrayOf` / `listOf` / `mutableListOf` 的内部重复 `Value T` 参数。
    ListForm,
    /// `Array<T>(size, initializer)` / `List<T>(size, initializer)`。
    RuntimeLength,
    /// `MutableList<T>()`。
    EmptyMutableList,
}

/// 一个已完成 Phase 2 检查的核心顺序容器构造。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerConstructionDescriptor {
    expression: ExpressionId,
    kind: ContainerConstructionKind,
    container: SequentialContainerKind,
    container_type: TypeId,
    element_type: TypeId,
    parameter_modes: Vec<ParameterMode>,
}

impl ContainerConstructionDescriptor {
    pub(crate) fn new(
        expression: ExpressionId,
        kind: ContainerConstructionKind,
        container: SequentialContainerKind,
        container_type: TypeId,
        element_type: TypeId,
        parameter_modes: Vec<ParameterMode>,
    ) -> Self {
        Self {
            expression,
            kind,
            container,
            container_type,
            element_type,
            parameter_modes,
        }
    }

    #[must_use]
    /// 返回拥有该构造的 call expression。
    pub const fn expression(&self) -> ExpressionId {
        self.expression
    }

    #[must_use]
    /// 返回封闭构造形状。
    pub const fn kind(&self) -> ContainerConstructionKind {
        self.kind
    }

    #[must_use]
    /// 返回构造出的顺序容器种类。
    pub const fn container(&self) -> SequentialContainerKind {
        self.container
    }

    #[must_use]
    /// 返回完整容器类型。
    pub const fn container_type(&self) -> TypeId {
        self.container_type
    }

    #[must_use]
    /// 返回保持不擦除的元素类型。
    pub const fn element_type(&self) -> TypeId {
        self.element_type
    }

    #[must_use]
    /// 返回源码实参顺序的固定参数模式。
    pub fn parameter_modes(&self) -> &[ParameterMode] {
        &self.parameter_modes
    }
}

/// 一个顺序容器下标表达式产生的 element place。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementPlaceDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    index: ExpressionId,
    container: SequentialContainerKind,
    element_type: TypeId,
}

impl ElementPlaceDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
        container: SequentialContainerKind,
        element_type: TypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            index,
            container,
            element_type,
        }
    }

    #[must_use]
    /// 返回产生该 place 的 index expression。
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    #[must_use]
    /// 返回只求值一次的 receiver expression。
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    #[must_use]
    /// 返回只求值一次的 index expression。
    pub const fn index(self) -> ExpressionId {
        self.index
    }

    #[must_use]
    /// 返回 receiver 的封闭容器种类。
    pub const fn container(self) -> SequentialContainerKind {
        self.container
    }

    #[must_use]
    /// 返回 element place 的值类型。
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    #[must_use]
    /// 返回该 element place 是否允许替换或独占借用。
    pub const fn is_mutable(self) -> bool {
        self.container.elements_are_mutable()
    }
}

impl TypedFile {
    /// 返回源码顺序的成功核心顺序容器构造。
    #[must_use]
    pub fn container_constructions(&self) -> &[ContainerConstructionDescriptor] {
        &self.container_constructions
    }

    /// 查询指定 call expression 的顺序容器构造描述符。
    #[must_use]
    pub fn container_construction(
        &self,
        expression: ExpressionId,
    ) -> Option<&ContainerConstructionDescriptor> {
        self.container_constructions
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码顺序的顺序容器 element place。
    #[must_use]
    pub fn element_places(&self) -> &[ElementPlaceDescriptor] {
        &self.element_places
    }

    /// 查询指定 index expression 的 element place。
    #[must_use]
    pub fn element_place(&self, expression: ExpressionId) -> Option<ElementPlaceDescriptor> {
        self.element_places
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
            .copied()
    }
}
