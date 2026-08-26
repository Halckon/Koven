use crate::{ast::ExpressionId, type_checking::TypeId};

/// Phase 3 已证明的 intrinsic `Rc<T>` owner effect。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RcOwnershipEffectKind {
    /// receiver 保持可用，同时产生一个新的 strong owner。
    Retain,
    /// payload 只通过 owner 约束的 shared read 暴露。
    BorrowPayload,
}

/// 可由 Phase 4 lowering 消费的 `Rc<T>` owner effect。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RcOwnershipEffect {
    expression: ExpressionId,
    receiver: ExpressionId,
    payload_type: TypeId,
    kind: RcOwnershipEffectKind,
}

impl RcOwnershipEffect {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        payload_type: TypeId,
        kind: RcOwnershipEffectKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            payload_type,
            kind,
        }
    }

    #[must_use]
    /// 返回产生结果的源码表达式。
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    #[must_use]
    /// 返回被读取但不消费的 shared-owner receiver。
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    #[must_use]
    /// 返回 control block payload 类型。
    pub const fn payload_type(self) -> TypeId {
        self.payload_type
    }

    #[must_use]
    /// 返回 retain 或 payload borrow effect。
    pub const fn kind(self) -> RcOwnershipEffectKind {
        self.kind
    }
}
