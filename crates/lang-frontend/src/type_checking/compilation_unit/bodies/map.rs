use super::{CompilationUnitTypes, UnitExpressionId, UnitTypeId};
use crate::type_checking::IntrinsicCallable;

/// compilation-unit 中经过 Phase 2 检查的 Map 构造调用 (`mapOf()` / `mutableMapOf()`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitMapConstructionDescriptor {
    expression: UnitExpressionId,
    callable: IntrinsicCallable,
    map_type: UnitTypeId,
    key_type: UnitTypeId,
    value_type: UnitTypeId,
}

impl UnitMapConstructionDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        callable: IntrinsicCallable,
        map_type: UnitTypeId,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
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
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回内建调用目标 (`MapOf` 或 `MutableMapOf`)。
    #[must_use]
    pub const fn callable(self) -> IntrinsicCallable {
        self.callable
    }

    /// 返回构造的目标 Map 类型标识。
    #[must_use]
    pub const fn map_type(self) -> UnitTypeId {
        self.map_type
    }

    /// 返回键类型标识。
    #[must_use]
    pub const fn key_type(self) -> UnitTypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> UnitTypeId {
        self.value_type
    }
}

/// compilation-unit 中 Map 的 `size` 属性访问描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitMapSizeDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    map_type: UnitTypeId,
}

impl UnitMapSizeDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        map_type: UnitTypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            map_type,
        }
    }

    /// 返回访问 `size` 属性的成员表达式节点。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回 Map receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回 Map 容器类型标识。
    #[must_use]
    pub const fn map_type(self) -> UnitTypeId {
        self.map_type
    }
}

/// compilation-unit 中 Map 的 `contains` 方法调用描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitMapContainsDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    key: UnitExpressionId,
    key_type: UnitTypeId,
}

impl UnitMapContainsDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        key: UnitExpressionId,
        key_type: UnitTypeId,
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
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回 Map receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回查找键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> UnitExpressionId {
        self.key
    }

    /// 返回查找键类型标识。
    #[must_use]
    pub const fn key_type(self) -> UnitTypeId {
        self.key_type
    }
}

/// compilation-unit 中 Map 的 `get` 方法或下标读取 `map[key]` 描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitMapGetDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    key: UnitExpressionId,
    key_type: UnitTypeId,
    value_type: UnitTypeId,
    result_type: UnitTypeId,
}

impl UnitMapGetDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        key: UnitExpressionId,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        result_type: UnitTypeId,
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
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回 Map receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回查询键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> UnitExpressionId {
        self.key
    }

    /// 返回查询键类型标识。
    #[must_use]
    pub const fn key_type(self) -> UnitTypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> UnitTypeId {
        self.value_type
    }

    /// 返回查询结果类型标识 (通常为 `V?`)。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.result_type
    }
}

/// compilation-unit 中 MutableMap 的 `put` 方法或下标赋值 `map[key] = value` 描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitMapPutDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    key: UnitExpressionId,
    value: UnitExpressionId,
    key_type: UnitTypeId,
    value_type: UnitTypeId,
}

impl UnitMapPutDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        key: UnitExpressionId,
        value: UnitExpressionId,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
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
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回 MutableMap receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回插入键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> UnitExpressionId {
        self.key
    }

    /// 返回插入值实参表达式节点。
    #[must_use]
    pub const fn value(self) -> UnitExpressionId {
        self.value
    }

    /// 返回键类型标识。
    #[must_use]
    pub const fn key_type(self) -> UnitTypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> UnitTypeId {
        self.value_type
    }
}

/// compilation-unit 中 MutableMap 的 `remove` 方法调用描述符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitMapRemoveDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    key: UnitExpressionId,
    key_type: UnitTypeId,
    value_type: UnitTypeId,
    result_type: UnitTypeId,
}

impl UnitMapRemoveDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        key: UnitExpressionId,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        result_type: UnitTypeId,
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
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回 MutableMap receiver 表达式节点。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回移除键实参表达式节点。
    #[must_use]
    pub const fn key(self) -> UnitExpressionId {
        self.key
    }

    /// 返回键类型标识。
    #[must_use]
    pub const fn key_type(self) -> UnitTypeId {
        self.key_type
    }

    /// 返回值类型标识。
    #[must_use]
    pub const fn value_type(self) -> UnitTypeId {
        self.value_type
    }

    /// 返回移除结果类型标识 (通常为 `V?`)。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.result_type
    }
}

/// compilation-unit 中所有 Map 相关的结构化操作描述符集合。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UnitMapDescriptors {
    pub(crate) constructions: Vec<UnitMapConstructionDescriptor>,
    pub(crate) sizes: Vec<UnitMapSizeDescriptor>,
    pub(crate) contains_calls: Vec<UnitMapContainsDescriptor>,
    pub(crate) gets: Vec<UnitMapGetDescriptor>,
    pub(crate) puts: Vec<UnitMapPutDescriptor>,
    pub(crate) removes: Vec<UnitMapRemoveDescriptor>,
}

impl CompilationUnitTypes {
    /// 返回编译单元中所有 Map 构造调用描述符。
    #[must_use]
    pub fn map_constructions(&self) -> &[UnitMapConstructionDescriptor] {
        &self.map_descriptors.constructions
    }

    /// 按表达式 ID 查询 Map 构造描述符。
    #[must_use]
    pub fn map_construction(
        &self,
        expression: UnitExpressionId,
    ) -> Option<&UnitMapConstructionDescriptor> {
        self.map_descriptors
            .constructions
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回编译单元中所有 Map `size` 访问描述符。
    #[must_use]
    pub fn map_sizes(&self) -> &[UnitMapSizeDescriptor] {
        &self.map_descriptors.sizes
    }

    /// 按表达式 ID 查询 Map `size` 访问描述符。
    #[must_use]
    pub fn map_size(&self, expression: UnitExpressionId) -> Option<&UnitMapSizeDescriptor> {
        self.map_descriptors
            .sizes
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回编译单元中所有 Map `contains` 调用描述符。
    #[must_use]
    pub fn map_contains_calls(&self) -> &[UnitMapContainsDescriptor] {
        &self.map_descriptors.contains_calls
    }

    /// 按表达式 ID 查询 Map `contains` 调用描述符。
    #[must_use]
    pub fn map_contains(&self, expression: UnitExpressionId) -> Option<&UnitMapContainsDescriptor> {
        self.map_descriptors
            .contains_calls
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回编译单元中所有 Map `get` 及下标读取描述符。
    #[must_use]
    pub fn map_gets(&self) -> &[UnitMapGetDescriptor] {
        &self.map_descriptors.gets
    }

    /// 按表达式 ID 查询 Map `get` 及下标读取描述符。
    #[must_use]
    pub fn map_get(&self, expression: UnitExpressionId) -> Option<&UnitMapGetDescriptor> {
        self.map_descriptors
            .gets
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回编译单元中所有 MutableMap `put` 及下标赋值描述符。
    #[must_use]
    pub fn map_puts(&self) -> &[UnitMapPutDescriptor] {
        &self.map_descriptors.puts
    }

    /// 按表达式 ID 查询 MutableMap `put` 及下标赋值描述符。
    #[must_use]
    pub fn map_put(&self, expression: UnitExpressionId) -> Option<&UnitMapPutDescriptor> {
        self.map_descriptors
            .puts
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }

    /// 返回编译单元中所有 MutableMap `remove` 调用描述符。
    #[must_use]
    pub fn map_removes(&self) -> &[UnitMapRemoveDescriptor] {
        &self.map_descriptors.removes
    }

    /// 按表达式 ID 查询 MutableMap `remove` 调用描述符。
    #[must_use]
    pub fn map_remove(&self, expression: UnitExpressionId) -> Option<&UnitMapRemoveDescriptor> {
        self.map_descriptors
            .removes
            .iter()
            .find(|descriptor| descriptor.expression() == expression)
    }
}
