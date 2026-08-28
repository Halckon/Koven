use super::{CompilationUnitTypes, UnitExpressionId, UnitTypeId};
use crate::type_checking::{ContainerConstructionKind, ParameterMode, SequentialContainerKind};

/// compilation-unit 中一个已完成 Phase 2 检查的核心顺序容器构造。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitContainerConstructionDescriptor {
    expression: UnitExpressionId,
    kind: ContainerConstructionKind,
    container: SequentialContainerKind,
    container_type: UnitTypeId,
    element_type: UnitTypeId,
    parameter_modes: Vec<ParameterMode>,
}

/// compilation-unit 中一个顺序容器下标表达式产生的 element place。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitElementPlaceDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    index: UnitExpressionId,
    container: SequentialContainerKind,
    element_type: UnitTypeId,
}

impl UnitElementPlaceDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        index: UnitExpressionId,
        container: SequentialContainerKind,
        element_type: UnitTypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            index,
            container,
            element_type,
        }
    }

    /// 返回带 source-unit 限定的 index expression identity。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回只求值一次的 index expression。
    #[must_use]
    pub const fn index(self) -> UnitExpressionId {
        self.index
    }

    /// 返回 receiver 的封闭容器种类。
    #[must_use]
    pub const fn container(self) -> SequentialContainerKind {
        self.container
    }

    /// 返回 element place 的 unit-global 值类型。
    #[must_use]
    pub const fn element_type(self) -> UnitTypeId {
        self.element_type
    }

    /// 返回该 element place 是否允许替换或独占借用。
    #[must_use]
    pub const fn is_mutable(self) -> bool {
        self.container.elements_are_mutable()
    }
}

impl UnitContainerConstructionDescriptor {
    pub(crate) fn new(
        expression: UnitExpressionId,
        kind: ContainerConstructionKind,
        container: SequentialContainerKind,
        container_type: UnitTypeId,
        element_type: UnitTypeId,
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

    /// 返回带 source-unit 限定的 call expression identity。
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }

    /// 返回封闭构造形状。
    #[must_use]
    pub const fn kind(&self) -> ContainerConstructionKind {
        self.kind
    }

    /// 返回构造出的顺序容器种类。
    #[must_use]
    pub const fn container(&self) -> SequentialContainerKind {
        self.container
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(&self) -> UnitTypeId {
        self.container_type
    }

    /// 返回未擦除的 unit-global 元素类型。
    #[must_use]
    pub const fn element_type(&self) -> UnitTypeId {
        self.element_type
    }

    /// 返回源码实参顺序的固定参数模式。
    #[must_use]
    pub fn parameter_modes(&self) -> &[ParameterMode] {
        &self.parameter_modes
    }
}

impl CompilationUnitTypes {
    /// 返回源码稳定顺序的核心顺序容器构造事实。
    #[must_use]
    pub fn container_constructions(&self) -> &[UnitContainerConstructionDescriptor] {
        &self.container_constructions
    }

    /// 查询一个成功 container call 的构造事实。
    #[must_use]
    pub fn container_construction(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitContainerConstructionDescriptor> {
        self.container_constructions
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的顺序容器 element place facts。
    #[must_use]
    pub fn element_places(&self) -> &[UnitElementPlaceDescriptor] {
        &self.element_places
    }

    /// 查询指定 source-qualified index expression 的 element place。
    #[must_use]
    pub fn element_place(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitElementPlaceDescriptor> {
        self.element_places
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }
}
