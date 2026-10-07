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
    /// `replace(&place, val)`.
    Replace,
    /// `swap(&a, &b)`.
    Swap,
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
    /// `Array<T>(size, initializer)` / `List<T>(size, initializer)` 的两个 Borrow 参数。
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

/// Phase 2 已识别的顺序容器同步 header 长度读取。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerSizeDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    container: SequentialContainerKind,
    container_type: TypeId,
    element_type: TypeId,
    result_type: TypeId,
    span: crate::source::Span,
}

impl ContainerSizeDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        container: SequentialContainerKind,
        container_type: TypeId,
        element_type: TypeId,
        result_type: TypeId,
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
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回 receiver 的封闭容器种类。
    #[must_use]
    pub const fn container(self) -> SequentialContainerKind {
        self.container
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> TypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// 返回独立的 Int 结果类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回完整读取表达式的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

impl TypedFile {
    /// 返回源码稳定顺序的容器长度读取事实。
    #[must_use]
    pub fn container_sizes(&self) -> &[ContainerSizeDescriptor] {
        &self.container_sizes
    }

    /// 查询成功识别的容器长度读取，不以成员拼写猜测身份。
    #[must_use]
    pub fn container_size(&self, expression: ExpressionId) -> Option<ContainerSizeDescriptor> {
        self.container_sizes
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.add 追加事实。
    #[must_use]
    pub fn container_appends(&self) -> &[ContainerAppendDescriptor] {
        &self.container_appends
    }

    /// 查询成功识别的 MutableList.add 追加描述符。
    #[must_use]
    pub fn container_append(&self, expression: ExpressionId) -> Option<ContainerAppendDescriptor> {
        self.container_appends
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.clear 清空事实。
    #[must_use]
    pub fn container_clears(&self) -> &[ContainerClearDescriptor] {
        &self.container_clears
    }

    /// 查询成功识别的 MutableList.clear 清空描述符。
    #[must_use]
    pub fn container_clear(&self, expression: ExpressionId) -> Option<ContainerClearDescriptor> {
        self.container_clears
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.removeAt 元素移出事实。
    #[must_use]
    pub fn container_remove_ats(&self) -> &[ContainerRemoveAtDescriptor] {
        &self.container_remove_ats
    }

    /// 查询成功识别的 MutableList.removeAt 元素移出描述符。
    #[must_use]
    pub fn container_remove_at(
        &self,
        expression: ExpressionId,
    ) -> Option<ContainerRemoveAtDescriptor> {
        self.container_remove_ats
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.removeLast 尾部元素移出事实。
    #[must_use]
    pub fn container_remove_lasts(&self) -> &[ContainerRemoveLastDescriptor] {
        &self.container_remove_lasts
    }

    /// 查询成功识别的 MutableList.removeLast 尾部元素移出描述符。
    #[must_use]
    pub fn container_remove_last(
        &self,
        expression: ExpressionId,
    ) -> Option<ContainerRemoveLastDescriptor> {
        self.container_remove_lasts
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.removeFirst 头部元素移出事实。
    #[must_use]
    pub fn container_remove_firsts(&self) -> &[ContainerRemoveFirstDescriptor] {
        &self.container_remove_firsts
    }

    /// 查询成功识别的 MutableList.removeFirst 头部元素移出描述符。
    #[must_use]
    pub fn container_remove_first(
        &self,
        expression: ExpressionId,
    ) -> Option<ContainerRemoveFirstDescriptor> {
        self.container_remove_firsts
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回源码稳定顺序的 MutableList.insertAt 元素插入事实。
    #[must_use]
    pub fn container_insert_ats(&self) -> &[ContainerInsertAtDescriptor] {
        &self.container_insert_ats
    }

    /// 查询成功识别的 MutableList.insertAt 元素插入描述符。
    #[must_use]
    pub fn container_insert_at(
        &self,
        expression: ExpressionId,
    ) -> Option<ContainerInsertAtDescriptor> {
        self.container_insert_ats
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }
}

/// Phase 2 已识别的 MutableList.clear 清空操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerClearDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    container_type: TypeId,
    element_type: TypeId,
    result_type: TypeId,
    span: crate::source::Span,
}

impl ContainerClearDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        container_type: TypeId,
        element_type: TypeId,
        result_type: TypeId,
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
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> TypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// 返回 Unit 结果类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

/// Phase 2 已识别的 MutableList.add 追加操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerAppendDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    element: ExpressionId,
    container_type: TypeId,
    element_type: TypeId,
    result_type: TypeId,
    span: crate::source::Span,
}

impl ContainerAppendDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        element: ExpressionId,
        container_type: TypeId,
        element_type: TypeId,
        result_type: TypeId,
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
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回被追加的 element expression identity。
    #[must_use]
    pub const fn element(self) -> ExpressionId {
        self.element
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> TypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// 返回 Unit 结果类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

/// Phase 2 已识别的 MutableList.removeAt 元素移出操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerRemoveAtDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    index: ExpressionId,
    container_type: TypeId,
    element_type: TypeId,
    result_type: TypeId,
    span: crate::source::Span,
}

impl ContainerRemoveAtDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
        container_type: TypeId,
        element_type: TypeId,
        result_type: TypeId,
        span: crate::source::Span,
    ) -> Self {
        Self {
            expression,
            receiver,
            index,
            container_type,
            element_type,
            result_type,
            span,
        }
    }

    /// 返回拥有该调用的 call expression identity。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回被移出位置的 index expression identity。
    #[must_use]
    pub const fn index(self) -> ExpressionId {
        self.index
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> TypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// 返回被移出元素的值类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

/// Phase 2 已识别的 MutableList.removeLast 尾部元素移出操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerRemoveLastDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    container_type: TypeId,
    element_type: TypeId,
    result_type: TypeId,
    span: crate::source::Span,
}

impl ContainerRemoveLastDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        container_type: TypeId,
        element_type: TypeId,
        result_type: TypeId,
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
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> TypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// 返回被移出元素的值类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

/// Phase 2 已识别的 MutableList.removeFirst 头部元素移出操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerRemoveFirstDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    container_type: TypeId,
    element_type: TypeId,
    result_type: TypeId,
    span: crate::source::Span,
}

impl ContainerRemoveFirstDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        container_type: TypeId,
        element_type: TypeId,
        result_type: TypeId,
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
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> TypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// 返回被移出元素的值类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}

/// Phase 2 已识别的 MutableList.insertAt 元素插入操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerInsertAtDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    index: ExpressionId,
    element: ExpressionId,
    container_type: TypeId,
    element_type: TypeId,
    result_type: TypeId,
    span: crate::source::Span,
}

impl ContainerInsertAtDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
        element: ExpressionId,
        container_type: TypeId,
        element_type: TypeId,
        result_type: TypeId,
        span: crate::source::Span,
    ) -> Self {
        Self {
            expression,
            receiver,
            index,
            element,
            container_type,
            element_type,
            result_type,
            span,
        }
    }

    /// 返回拥有该调用的 call expression identity。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回只求值一次的 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回插入位置的 index expression identity。
    #[must_use]
    pub const fn index(self) -> ExpressionId {
        self.index
    }

    /// 返回插入的 element expression identity。
    #[must_use]
    pub const fn element(self) -> ExpressionId {
        self.element
    }

    /// 返回完整容器类型。
    #[must_use]
    pub const fn container_type(self) -> TypeId {
        self.container_type
    }

    /// 返回保持不擦除的元素类型。
    #[must_use]
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// 返回 Unit 结果类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    /// 返回调用的源码范围。
    #[must_use]
    pub const fn span(self) -> crate::source::Span {
        self.span
    }
}
