use crate::{
    source::Span,
    type_checking::{
        BuiltinType, ParameterMode, StringOperationKind, UnitCallableTarget, UnitExpressionId,
        UnitTypeId,
    },
};

/// Phase 2 为可到达的插值操作数选定的显示转换。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitDisplayTarget {
    /// 由内建类型决定的显示转换。
    Builtin(BuiltinType),
    /// 标准 Display.toString 的静态协议成员，实例化后使用已有的静态分派合同。
    Requirement(UnitCallableTarget),
}

/// 一个按源码顺序执行的插值步骤。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitInterpolationPart {
    /// 需解码并追加的源码文本。
    Text(Span),
    /// 一次求值后按 Borrow receiver 进行转换。
    Format {
        /// 带 source identity 的操作数。
        operand: UnitExpressionId,
        /// 已选定的显示转换。
        target: UnitDisplayTarget,
    },
    /// 只求值。该操作数必然离开当前表达式，不进行转换或后续步骤。
    Exit(UnitExpressionId),
}

/// Phase 2 已确认 builtin String receiver 的显式复制操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitStringOperationDescriptor {
    expression: UnitExpressionId,
    receiver: UnitExpressionId,
    receiver_type: UnitTypeId,
}
impl UnitStringOperationDescriptor {
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
    /// 返回产生独立 owner 的调用表达式。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }
    /// 返回调用期共享借用的 receiver 表达式。
    #[must_use]
    pub const fn receiver(self) -> UnitExpressionId {
        self.receiver
    }
    /// 返回已绑定的 builtin String 类型。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }
    /// 返回新 owner 的 String 类型。
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
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
