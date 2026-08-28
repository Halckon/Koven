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
}
