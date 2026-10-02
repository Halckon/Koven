use super::TypeId;
use crate::ast::ExpressionId;

/// 编译器绑定的整数操作身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerOperationKind {
    /// 在接收者类型位宽内逐位取反。
    Invert,
}

/// Phase 2 已确认整数 receiver 的具名操作；下游不再解释成员拼写。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerOperationDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    receiver_type: TypeId,
}
impl IntegerOperationDescriptor {
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
    /// 返回调用表达式的稳定身份。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }
    /// 返回仅求值一次的整数接收者。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }
    /// 返回已绑定的整数类型。
    #[must_use]
    pub const fn receiver_type(self) -> TypeId {
        self.receiver_type
    }
    /// 返回保持位宽和符号的结果类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.receiver_type
    }
    /// 返回稳定 intrinsic 身份。
    #[must_use]
    pub const fn kind(self) -> IntegerOperationKind {
        IntegerOperationKind::Invert
    }
}
