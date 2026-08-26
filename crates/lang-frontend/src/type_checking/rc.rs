use crate::ast::ExpressionId;

use super::{ParameterMode, TypeId};

/// 编译器绑定的 `Rc<T>` 操作身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RcOperationKind {
    /// 显式增加一个 strong owner，并产生新的 owned handle。
    Share,
    /// 在 owner 存活期间共享借用 payload。
    Value,
}

/// Phase 2 已确认 receiver identity 的 intrinsic `Rc<T>` 操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RcOperationDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    payload_type: TypeId,
    kind: RcOperationKind,
}

impl RcOperationDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        payload_type: TypeId,
        kind: RcOperationKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            payload_type,
            kind,
        }
    }

    /// 返回产生该操作结果的源码表达式。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回持有 shared owner 的 receiver 表达式。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回 control block 中 payload 的规范化类型。
    #[must_use]
    pub const fn payload_type(self) -> TypeId {
        self.payload_type
    }

    /// 返回内建操作身份。
    #[must_use]
    pub const fn kind(self) -> RcOperationKind {
        self.kind
    }

    /// 返回操作结果的所有权 effect；`.share()` 产生 Value，`.value` 只产生 Borrow。
    #[must_use]
    pub const fn result_mode(self) -> ParameterMode {
        match self.kind {
            RcOperationKind::Share => ParameterMode::Value,
            RcOperationKind::Value => ParameterMode::Borrow,
        }
    }
}
