//! source-qualified `!!` 类型事实；整体包含于 overload trial 的 nullable facts 快照。
use super::CompilationUnitTypes;
use crate::{
    source::Span,
    type_checking::{
        AssertionFailureEffect, Copyability, ExpressionCategory, NullableWhenSubjectCategory,
        UnitExpressionId, UnitTypeId,
    },
};

/// 一个 assertion 与同一次 operand 求值绑定的类型事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitNonNullAssertionDescriptor {
    pub(crate) expression: UnitExpressionId,
    pub(crate) operand: UnitExpressionId,
    pub(crate) operator_span: Span,
    pub(crate) nullable_type: UnitTypeId,
    pub(crate) inner_type: UnitTypeId,
    pub(crate) category: ExpressionCategory,
    pub(crate) source_category: NullableWhenSubjectCategory,
    pub(crate) copyability: Copyability,
}

impl UnitNonNullAssertionDescriptor {
    /// `NonNullAssert` AST identity，同时唯一标识其 compiler-bound Abort。
    #[must_use]
    pub fn expression(&self) -> UnitExpressionId {
        self.expression
    }
    /// 后续阶段必须复用的唯一 operand 求值身份。
    #[must_use]
    pub fn operand(&self) -> UnitExpressionId {
        self.operand
    }
    /// `!!` 的源码位置。
    #[must_use]
    pub fn operator_span(&self) -> Span {
        self.operator_span
    }
    /// operand 的 nullable 类型。
    #[must_use]
    pub fn nullable_type(&self) -> UnitTypeId {
        self.nullable_type
    }
    /// 成功 edge 的结果类型。
    #[must_use]
    pub fn inner_type(&self) -> UnitTypeId {
        self.inner_type
    }
    /// operand 的 place/temporary 类别。
    #[must_use]
    pub fn category(&self) -> ExpressionCategory {
        self.category
    }
    /// 与 nullable when 共用的 root binding/field/element 来源类别。
    #[must_use]
    pub fn source_category(&self) -> NullableWhenSubjectCategory {
        self.source_category
    }
    /// Copyable 表示复制候选，MoveOnly 表示整体消费候选；不证明 move/loan/drop 合法。
    #[must_use]
    pub fn copyability(&self) -> Copyability {
        self.copyability
    }
    /// 由 assertion AST 绑定，不查询名为 error 的声明。
    #[must_use]
    pub fn failure_effect(&self) -> AssertionFailureEffect {
        AssertionFailureEffect::Abort
    }
}

impl CompilationUnitTypes {
    /// 返回按 source-qualified assertion identity 排序的类型描述符。
    #[must_use]
    pub fn non_null_assertions(&self) -> &[UnitNonNullAssertionDescriptor] {
        &self.nullable.non_null_assertions
    }
    /// 查询绑定同一次 operand 求值的 assertion 类型事实。
    #[must_use]
    pub fn non_null_assertion(
        &self,
        expression: UnitExpressionId,
    ) -> Option<UnitNonNullAssertionDescriptor> {
        self.nullable
            .non_null_assertions
            .iter()
            .copied()
            .find(|descriptor| descriptor.expression() == expression)
    }
}
