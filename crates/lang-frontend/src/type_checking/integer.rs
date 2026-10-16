use super::{BuiltinType, TypeId};
use crate::ast::ExpressionId;

/// 编译器绑定的整数操作身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerOperationKind {
    /// 在接收者类型位宽内逐位取反。
    Invert,
    /// 按目标整数数学值域检查后转换；越界沿既有 Abort。
    CheckedConvert,
}

/// Phase 2 已确认整数 receiver 的具名操作；下游不再解释成员拼写。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerOperationDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    receiver_type: TypeId,
    result_type: TypeId,
    kind: IntegerOperationKind,
}
impl IntegerOperationDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_type: TypeId,
        result_type: TypeId,
        kind: IntegerOperationKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            receiver_type,
            result_type,
            kind,
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
    /// 返回操作的目标结果类型。
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }
    /// 返回稳定 intrinsic 身份。
    #[must_use]
    pub const fn kind(self) -> IntegerOperationKind {
        self.kind
    }
}

/// compiler-bound 成员候选；仅在真实整数 receiver 上取得此身份。
pub(crate) fn integer_member(name: &str) -> Option<(IntegerOperationKind, Option<BuiltinType>)> {
    let target = match name {
        "inv" => return Some((IntegerOperationKind::Invert, None)),
        "toByte" => BuiltinType::Byte,
        "toShort" => BuiltinType::Short,
        "toInt" => BuiltinType::Int,
        "toLong" => BuiltinType::Long,
        "toUByte" => BuiltinType::UByte,
        "toUShort" => BuiltinType::UShort,
        "toUInt" => BuiltinType::UInt,
        "toULong" => BuiltinType::ULong,
        _ => return None,
    };
    Some((IntegerOperationKind::CheckedConvert, Some(target)))
}

pub(crate) fn is_integer_builtin(builtin: BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Byte
            | BuiltinType::Short
            | BuiltinType::Int
            | BuiltinType::Long
            | BuiltinType::UByte
            | BuiltinType::UShort
            | BuiltinType::UInt
            | BuiltinType::ULong
    )
}
