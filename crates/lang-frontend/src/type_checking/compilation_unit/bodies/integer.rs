use crate::type_checking::{IntegerOperationKind, UnitExpressionId, UnitTypeId};

/// Phase 2 已确认整数 receiver 的具名操作；下游不再解释成员拼写。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitIntegerOperationDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    receiver_type: UnitTypeId,
}
impl UnitIntegerOperationDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitExpressionId,
        receiver_type: UnitTypeId,
    ) -> Self {
        Self {
            expression,
            receiver,
            receiver_type,
        }
    }
    /// 返回调用表达式的稳定身份。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }
    /// 返回仅求值一次的整数接收者。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }
    /// 返回已绑定的整数类型。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }
    /// 返回保持位宽和符号的结果类型。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.receiver_type
    }
    /// 返回稳定 intrinsic 身份。
    #[must_use]
    pub const fn kind(self) -> IntegerOperationKind {
        IntegerOperationKind::Invert
    }
}

impl super::CompilationUnitTypes {
    pub(super) fn integer_operations_are_valid(&self) -> bool {
        use crate::type_checking::{BuiltinType, ExpressionCategory, UnitTypeKind};
        let mut previous = None;
        self.integer_operations.iter().all(|fact| {
            let ordered = previous.is_none_or(|expression| expression < fact.expression);
            previous = Some(fact.expression);
            ordered
                && fact.expression.source_unit() == fact.receiver.source_unit()
                && fact.expression != fact.receiver
                && self.expression_type(fact.expression) == Some(fact.receiver_type)
                && self.expression_type(fact.receiver) == Some(fact.receiver_type)
                && self.expression_category(fact.expression) == Some(ExpressionCategory::Temporary)
                && matches!(
                    self.types().get(fact.receiver_type),
                    Some(UnitTypeKind::Builtin(
                        BuiltinType::Byte
                            | BuiltinType::Short
                            | BuiltinType::Int
                            | BuiltinType::Long
                            | BuiltinType::UByte
                            | BuiltinType::UShort
                            | BuiltinType::UInt
                            | BuiltinType::ULong
                    ))
                )
        })
    }
}
