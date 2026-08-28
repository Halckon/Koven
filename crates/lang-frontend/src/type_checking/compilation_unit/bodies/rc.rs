use crate::type_checking::{ParameterMode, RcOperationKind, UnitExpressionId, UnitTypeId};

/// Phase 2 已确认 receiver identity 的 compilation-unit intrinsic `Rc<T>` 操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitRcOperationDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    payload_type: UnitTypeId,
    kind: RcOperationKind,
}

impl UnitRcOperationDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        payload_type: UnitTypeId,
        kind: RcOperationKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            payload_type,
            kind,
        }
    }

    /// 返回产生该操作结果的 source-qualified expression。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回持有 shared owner 的 source-qualified receiver。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }

    /// 返回 control block 中 payload 的 unit-global 类型。
    #[must_use]
    pub const fn payload_type(self) -> UnitTypeId {
        self.payload_type
    }

    /// 返回 compiler-bound `share` / `value` 操作身份。
    #[must_use]
    pub const fn kind(self) -> RcOperationKind {
        self.kind
    }

    /// 返回结果交付模式：`.share()` 为 Value，`.value` 为 Borrow。
    #[must_use]
    pub const fn result_mode(self) -> ParameterMode {
        match self.kind {
            RcOperationKind::Share => ParameterMode::Value,
            RcOperationKind::Value => ParameterMode::Borrow,
        }
    }
}
