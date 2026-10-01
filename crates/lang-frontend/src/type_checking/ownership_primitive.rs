use crate::ast::ExpressionId;

use super::TypeId;

/// 编译器绑定的原子所有权置换身份；源码同名函数不获得此身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipPrimitiveKind {
    /// 将新 owner 存入独占 place，并返回旧 owner。
    Replace,
    /// 交换两个独占且不重叠的 place 的 owner。
    Swap,
}

/// Phase 2 的静态原语结构；不代表 Phase 3 已准许或执行原子 commit。
///
/// 即使 operand 不正常返回，也保留此事实，供后继阶段识别并求值调用前缀。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnershipPrimitiveDescriptor {
    expression: ExpressionId,
    kind: OwnershipPrimitiveKind,
    value_type: TypeId,
    operands: [ExpressionId; 2],
}

impl OwnershipPrimitiveDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        kind: OwnershipPrimitiveKind,
        value_type: TypeId,
        operands: [ExpressionId; 2],
    ) -> Self {
        Self {
            expression,
            kind,
            value_type,
            operands,
        }
    }

    /// 返回原语调用表达式。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回 compiler-bound 原语身份。
    #[must_use]
    pub const fn kind(self) -> OwnershipPrimitiveKind {
        self.kind
    }

    /// 返回被置换值的规范化类型 `T`。
    #[must_use]
    pub const fn value_type(self) -> TypeId {
        self.value_type
    }

    /// 返回源码求值顺序的两个实参；replace 为 place/new，swap 为 a/b。
    #[must_use]
    pub const fn operands(self) -> [ExpressionId; 2] {
        self.operands
    }
}
