use crate::ast::ExpressionId;

use super::{ParameterMode, TypeId};

/// 编译器绑定的 String 操作身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringOperationKind {
    /// 共享借用源 UTF-8 字节，产生独立的新 String owner。
    Clone,
}

/// Phase 2 已确认 builtin String receiver 的显式复制操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StringOperationDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    receiver_type: TypeId,
}
impl StringOperationDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_type: TypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            receiver_type,
        }
    }
    /// 返回产生独立 owner 的调用表达式。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }
    /// 返回调用期共享借用的 receiver 表达式。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }
    /// 返回已绑定的 builtin String 类型。
    #[must_use]
    pub const fn receiver_type(self) -> TypeId {
        self.receiver_type
    }
    /// 返回新 owner 的 String 类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.receiver_type
    }
    /// 返回稳定 intrinsic 身份，不由下游重新解释成员名。
    #[must_use]
    pub const fn kind(self) -> StringOperationKind {
        StringOperationKind::Clone
    }
    /// receiver 在同步调用期间只读借用。
    #[must_use]
    pub const fn receiver_mode(self) -> ParameterMode {
        ParameterMode::Borrow
    }
    /// 返回值承担独立析构义务。
    #[must_use]
    pub const fn result_mode(self) -> ParameterMode {
        ParameterMode::Value
    }
}
