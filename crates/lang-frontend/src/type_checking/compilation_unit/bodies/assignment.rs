use crate::{parser::AssignmentOperator, type_checking::UnitTypeId};

use super::UnitExpressionId;

/// compilation unit 中一个已完成类型验证的普通替换赋值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitAssignmentDescriptor {
    expression: UnitExpressionId,
    target: UnitExpressionId,
    value: UnitExpressionId,
    operator: AssignmentOperator,
    target_type: UnitTypeId,
    falls_through: bool,
}

impl UnitAssignmentDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        target: UnitExpressionId,
        value: UnitExpressionId,
        operator: AssignmentOperator,
        target_type: UnitTypeId,
        falls_through: bool,
    ) -> Self {
        Self {
            expression,
            target,
            value,
            operator,
            target_type,
            falls_through,
        }
    }

    /// 返回赋值表达式的 source-qualified identity。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回 target expression 的 source-qualified identity。
    #[must_use]
    pub const fn target(self) -> UnitExpressionId {
        self.target
    }

    /// 返回 RHS expression 的 source-qualified identity。
    #[must_use]
    pub const fn value(self) -> UnitExpressionId {
        self.value
    }

    /// 返回源码赋值运算符；本 Spec 只发布普通 `=`。
    #[must_use]
    pub const fn operator(self) -> AssignmentOperator {
        self.operator
    }

    /// 返回 target place 的声明/storage 类型，而不是 flow-narrowed expression 类型。
    #[must_use]
    pub const fn target_type(self) -> UnitTypeId {
        self.target_type
    }

    /// 返回 target 与 RHS 合并后的正常继续控制事实。
    #[must_use]
    pub const fn falls_through(self) -> bool {
        self.falls_through
    }
}
