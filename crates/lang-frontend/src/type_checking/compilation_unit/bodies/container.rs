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

/// Phase 2 已识别的顺序容器同步 header 长度读取。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitContainerSizeDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    container: SequentialContainerKind,
    container_type: UnitTypeId,
    element_type: UnitTypeId,
    result_type: UnitTypeId,
    span: crate::source::Span,
}

impl UnitContainerSizeDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        container: SequentialContainerKind,
        container_type: UnitTypeId,
        element_type: UnitTypeId,
        result_type: UnitTypeId,
        span: crate::source::Span,
    ) -> Self {
        Self {
            expression,
            receiver,
            container,
            container_type,
            element_type,
            result_type,
            span,
        }
    }

    /// 返回拥有该读取的 member expression identity。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回 receiver 的封闭容器种类。
    #[must_use]
    pub const fn container(self) -> SequentialContainerKind {
        self.container
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> UnitTypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> UnitTypeId {
        self.element_type
    }

    /// 返回独立的 Int 结果类型。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.result_type
    }

    /// 返回完整读取表达式的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

impl CompilationUnitTypes {
    /// 返回源码稳定顺序的容器长度读取事实。
    #[must_use]
    pub fn container_sizes(&self) -> &[UnitContainerSizeDescriptor] {
        &self.container_sizes
    }

    /// 查询成功识别的容器长度读取，不以成员拼写猜测身份。
    #[must_use]
    pub fn container_size(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitContainerSizeDescriptor> {
        self.container_sizes
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.add 追加事实。
    #[must_use]
    pub fn container_appends(&self) -> &[UnitContainerAppendDescriptor] {
        &self.container_appends
    }

    /// 查询成功识别的 MutableList.add 追加描述符。
    #[must_use]
    pub fn container_append(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitContainerAppendDescriptor> {
        self.container_appends
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.clear 清空事实。
    #[must_use]
    pub fn container_clears(&self) -> &[UnitContainerClearDescriptor] {
        &self.container_clears
    }

    /// 查询成功识别的 MutableList.clear 清空描述符。
    #[must_use]
    pub fn container_clear(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitContainerClearDescriptor> {
        self.container_clears
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }
}

/// Compilation unit 级别的 MutableList.add 追加描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitContainerAppendDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    element: UnitExpressionId,
    container_type: UnitTypeId,
    element_type: UnitTypeId,
    result_type: UnitTypeId,
    span: crate::source::Span,
}

impl UnitContainerAppendDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        element: UnitExpressionId,
        container_type: UnitTypeId,
        element_type: UnitTypeId,
        result_type: UnitTypeId,
        span: crate::source::Span,
    ) -> Self {
        Self {
            expression,
            receiver,
            element,
            container_type,
            element_type,
            result_type,
            span,
        }
    }

    /// 返回拥有该调用的 call expression identity。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回被追加的 element expression identity。
    #[must_use]
    pub const fn element(self) -> UnitExpressionId {
        self.element
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> UnitTypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> UnitTypeId {
        self.element_type
    }

    /// 返回 Unit 结果类型。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

/// Compilation unit 级别的 MutableList.clear 清空描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitContainerClearDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    container_type: UnitTypeId,
    element_type: UnitTypeId,
    result_type: UnitTypeId,
    span: crate::source::Span,
}

impl UnitContainerClearDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        container_type: UnitTypeId,
        element_type: UnitTypeId,
        result_type: UnitTypeId,
        span: crate::source::Span,
    ) -> Self {
        Self {
            expression,
            receiver,
            container_type,
            element_type,
            result_type,
            span,
        }
    }

    /// 返回拥有该调用的 call expression identity。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> UnitTypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> UnitTypeId {
        self.element_type
    }

    /// 返回 Unit 结果类型。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}
