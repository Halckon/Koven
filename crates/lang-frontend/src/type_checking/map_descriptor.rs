use super::{IntrinsicCallable, TypeId, TypedFile};
use crate::ast::ExpressionId;

/// 单文件中经过 Phase 2 类型检查的 Map 构造调用 (`mapOf()` / `mutableMapOf()`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapConstructionDescriptor {
    expression: ExpressionId,
    callable: IntrinsicCallable,
    map_type: TypeId,
    key_type: TypeId,
    value_type: TypeId,
}

impl MapConstructionDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        callable: IntrinsicCallable,
        map_type: TypeId,
        key_type: TypeId,
        value_type: TypeId,
    ) -> Self {
        Self {
            expression,
            callable,
            map_type,
            key_type,
            value_type,
        }
    }

    /// 返回 Map 构造调用的表达式节点。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回内建调用目标 (`MapOf` 或 `MutableMapOf`)。
    #[must_use]
    pub const fn callable(self) -> IntrinsicCallable {
        self.callable
    }

    /// 返回构造的目标 Map 类型标识。
    #[must_use]
    pub const fn map_type(self) -> TypeId {
        self.map_type
    }

    /// 返回键类型标识。
    #[must_use]
    pub const fn key_type(self) -> TypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> TypeId {
        self.value_type
    }
}

/// 单文件中 Map 的 `size` 属性访问描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapSizeDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    map_type: TypeId,
}

impl MapSizeDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        map_type: TypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            map_type,
        }
    }

    /// 返回访问 `size` 属性的成员表达式节点。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回 Map receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回 Map 容器类型标识。
    #[must_use]
    pub const fn map_type(self) -> TypeId {
        self.map_type
    }
}

/// 单文件中 Map 的 `contains` 方法调用描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapContainsDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    key: ExpressionId,
    key_type: TypeId,
}

impl MapContainsDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        key: ExpressionId,
        key_type: TypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            key,
            key_type,
        }
    }

    /// 返回 `contains` 调用的表达式节点。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回 Map receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回查找键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> ExpressionId {
        self.key
    }

    /// 返回查找键类型标识。
    #[must_use]
    pub const fn key_type(self) -> TypeId {
        self.key_type
    }
}

/// 单文件中 Map 的 `get` 方法或下标读取 `map[key]` 描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapGetDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    key: ExpressionId,
    key_type: TypeId,
    value_type: TypeId,
    result_type: TypeId,
}

impl MapGetDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        key: ExpressionId,
        key_type: TypeId,
        value_type: TypeId,
        result_type: TypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            key,
            key_type,
            value_type,
            result_type,
        }
    }

    /// 返回 `get` 调用或下标表达式节点。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回 Map receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回查询键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> ExpressionId {
        self.key
    }

    /// 返回查询键类型标识。
    #[must_use]
    pub const fn key_type(self) -> TypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> TypeId {
        self.value_type
    }

    /// 返回查询结果类型标识 (通常为 `V?`)。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }
}

/// 单文件中 MutableMap 的 `put` 方法或下标赋值 `map[key] = value` 描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapPutDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    key: ExpressionId,
    value: ExpressionId,
    key_type: TypeId,
    value_type: TypeId,
}

impl MapPutDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        key: ExpressionId,
        value: ExpressionId,
        key_type: TypeId,
        value_type: TypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            key,
            value,
            key_type,
            value_type,
        }
    }

    /// 返回 `put` 调用或赋值表达式节点。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回 MutableMap receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回插入键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> ExpressionId {
        self.key
    }

    /// 返回插入值实参表达式节点。
    #[must_use]
    pub const fn value(self) -> ExpressionId {
        self.value
    }

    /// 返回键类型标识。
    #[must_use]
    pub const fn key_type(self) -> TypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> TypeId {
        self.value_type
    }
}

/// 单文件中 MutableMap 的 `remove` 方法调用描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapRemoveDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    key: ExpressionId,
    key_type: TypeId,
    value_type: TypeId,
    result_type: TypeId,
}

impl MapRemoveDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        key: ExpressionId,
        key_type: TypeId,
        value_type: TypeId,
        result_type: TypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            key,
            key_type,
            value_type,
            result_type,
        }
    }

    /// 返回 `remove` 调用的表达式节点。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回 MutableMap receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回移除键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> ExpressionId {
        self.key
    }

    /// 返回键类型标识。
    #[must_use]
    pub const fn key_type(self) -> TypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> TypeId {
        self.value_type
    }

    /// 返回移除结果类型标识 (通常为 `V?`)。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }
}

/// 确定存在的 Map 槽位借用，source-qualified unit 使用同一封闭合同。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapRequireValueDescriptor<E = ExpressionId, T = TypeId> {
    expression: E,
    receiver: E,
    key: E,
    key_type: T,
    value_type: T,
    borrow_return: super::BorrowReturnContract,
}
impl<E: Copy, T: Copy> MapRequireValueDescriptor<E, T> {
    pub(crate) const fn new(
        expression: E,
        receiver: E,
        key: E,
        key_type: T,
        value_type: T,
        marker: crate::source::Span,
        source: crate::source::Span,
    ) -> Self {
        Self {
            expression,
            receiver,
            key,
            key_type,
            value_type,
            borrow_return: super::BorrowReturnContract::intrinsic_receiver(marker, source),
        }
    }
    /// 查询调用 identity。
    pub const fn expression(self) -> E {
        self.expression
    }
    /// 唯一来源 receiver。
    pub const fn receiver(self) -> E {
        self.receiver
    }
    /// 同步 Borrow 的 key。
    pub const fn key(self) -> E {
        self.key
    }
    /// 已验证的 key 类型。
    pub const fn key_type(self) -> T {
        self.key_type
    }
    /// 槽位的原始 V 类型，不增加 nullable 层。
    pub const fn value_type(self) -> T {
        self.value_type
    }
    /// 唯一 from receiver 的只读交付合同。
    pub const fn borrow_return(self) -> super::BorrowReturnContract {
        self.borrow_return
    }
}

/// 单文件中所有 Map 相关的结构化操作描述符集合。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MapDescriptors {
    pub(crate) constructions: Vec<MapConstructionDescriptor>,
    pub(crate) sizes: Vec<MapSizeDescriptor>,
    pub(crate) contains_calls: Vec<MapContainsDescriptor>,
    pub(crate) gets: Vec<MapGetDescriptor>,
    pub(crate) requires: Vec<MapRequireValueDescriptor>,
    pub(crate) with_values: Vec<super::MapWithValueDescriptor>,
    pub(crate) puts: Vec<MapPutDescriptor>,
    pub(crate) removes: Vec<MapRemoveDescriptor>,
}

impl TypedFile {
    /// 文件中的确定槽位借用查询。
    pub fn map_require_values(&self) -> &[MapRequireValueDescriptor] {
        &self.map_descriptors.requires
    }
    /// 按实际调用 identity 查询确定槽位借用。
    pub fn map_require_value(
        &self,
        expression: ExpressionId,
    ) -> Option<&MapRequireValueDescriptor> {
        self.map_descriptors
            .requires
            .iter()
            .find(|d| d.expression() == expression)
    }
    /// 普通与内建调用的借用返回合同。
    pub fn call_borrow_return(
        &self,
        expression: ExpressionId,
    ) -> Option<super::BorrowReturnContract> {
        self.map_require_value(expression)
            .map(|d| d.borrow_return())
            .or_else(|| self.call(expression).and_then(|d| d.borrow_return()))
    }

    /// 返回文件中所有 Map 构造调用描述符。
    #[must_use]
    pub fn map_constructions(&self) -> &[MapConstructionDescriptor] {
        &self.map_descriptors.constructions
    }

    /// 按表达式 ID 查询 Map 构造描述符。
    #[must_use]
    pub fn map_construction(&self, expression: ExpressionId) -> Option<&MapConstructionDescriptor> {
        self.map_descriptors
            .constructions
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回文件中所有 Map `size` 访问描述符。
    #[must_use]
    pub fn map_sizes(&self) -> &[MapSizeDescriptor] {
        &self.map_descriptors.sizes
    }

    /// 按表达式 ID 查询 Map `size` 访问描述符。
    #[must_use]
    pub fn map_size(&self, expression: ExpressionId) -> Option<&MapSizeDescriptor> {
        self.map_descriptors
            .sizes
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回文件中所有 Map `contains` 调用描述符。
    #[must_use]
    pub fn map_contains_calls(&self) -> &[MapContainsDescriptor] {
        &self.map_descriptors.contains_calls
    }

    /// 按表达式 ID 查询 Map `contains` 调用描述符。
    #[must_use]
    pub fn map_contains(&self, expression: ExpressionId) -> Option<&MapContainsDescriptor> {
        self.map_descriptors
            .contains_calls
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回文件中所有 Map `get` 及下标读取描述符。
    #[must_use]
    pub fn map_gets(&self) -> &[MapGetDescriptor] {
        &self.map_descriptors.gets
    }

    /// 按表达式 ID 查询 Map `get` 及下标读取描述符。
    #[must_use]
    pub fn map_get(&self, expression: ExpressionId) -> Option<&MapGetDescriptor> {
        self.map_descriptors
            .gets
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回文件中所有 MutableMap `put` 及下标赋值描述符。
    #[must_use]
    pub fn map_puts(&self) -> &[MapPutDescriptor] {
        &self.map_descriptors.puts
    }

    /// 按表达式 ID 查询 MutableMap `put` 及下标赋值描述符。
    #[must_use]
    pub fn map_put(&self, expression: ExpressionId) -> Option<&MapPutDescriptor> {
        self.map_descriptors
            .puts
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回文件中所有 MutableMap `remove` 调用描述符。
    #[must_use]
    pub fn map_removes(&self) -> &[MapRemoveDescriptor] {
        &self.map_descriptors.removes
    }

    /// 按表达式 ID 查询 MutableMap `remove` 调用描述符。
    #[must_use]
    pub fn map_remove(&self, expression: ExpressionId) -> Option<&MapRemoveDescriptor> {
        self.map_descriptors
            .removes
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }
}
