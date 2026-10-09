//! 同步且非逃逸的 Map callback 访问合同。
use super::{TypeId, TypedFile};
use crate::ast::ExpressionId;
/// 已验证的同步作用域访问；action 参数是 Borrow V 且结果为 Unit。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapWithValueDescriptor<E = ExpressionId, T = TypeId> {
    expression: E,
    receiver: E,
    key: E,
    action: E,
    key_type: T,
    value_type: T,
    action_type: T,
}
impl<E: Copy, T: Copy> MapWithValueDescriptor<E, T> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        expression: E,
        receiver: E,
        key: E,
        action: E,
        key_type: T,
        value_type: T,
        action_type: T,
    ) -> Self {
        Self {
            expression,
            receiver,
            key,
            action,
            key_type,
            value_type,
            action_type,
        }
    }
    /// 返回访问调用 identity。
    pub const fn expression(self) -> E {
        self.expression
    }
    /// 返回Map 来源。
    pub const fn receiver(self) -> E {
        self.receiver
    }
    /// 返回同步只读键。
    pub const fn key(self) -> E {
        self.key
    }
    /// 返回同步非逃逸 callback。
    pub const fn action(self) -> E {
        self.action
    }
    /// 返回同步只读键。
    /// 返回键类型。
    pub const fn key_type(self) -> T {
        self.key_type
    }
    /// 返回槽位类型。
    pub const fn value_type(self) -> T {
        self.value_type
    }
    /// 返回同步非逃逸 callback。
    /// 返回callback 的预期签名。
    pub const fn action_type(self) -> T {
        self.action_type
    }
}
impl TypedFile {
    /// 文件中的作用域访问描述符。
    pub fn map_with_values(&self) -> &[MapWithValueDescriptor] {
        &self.map_descriptors.with_values
    }
    /// 查询指定调用的作用域访问描述符。
    pub fn map_with_value(&self, expression: ExpressionId) -> Option<&MapWithValueDescriptor> {
        self.map_descriptors
            .with_values
            .iter()
            .find(|d| d.expression() == expression)
    }
}
