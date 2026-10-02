//! 单文件无 payload Copyable enum case 条件只消费已验证的构造事实。
use lang_frontend::{
    ast::ExpressionId,
    type_checking::{BuiltinType, ConstructionTarget, Copyability, TypeKind},
};

use super::{ExpressionLowerer, LoweringError, LoweringErrorKind, error, value};
use crate::ssa::model::{
    ComparisonOperator, EntityType, Operation, ScalarConstant, SsaTypeId, ValueId,
};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_copyable_enum_case_condition(
        &mut self,
        owner: ValueId,
        tagged: SsaTypeId,
        expression: ExpressionId,
    ) -> Result<ValueId, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .construction(expression)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let ConstructionTarget::EnumCase(case_id) = descriptor.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !descriptor.arguments().is_empty()
            || self.typed.copyability(descriptor.result_type()) != Some(Copyability::Copyable)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let case = self
            .typed
            .enum_cases()
            .iter()
            .find(|case| case.id() == case_id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !case.payloads().is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if descriptor.expression() != expression
            || self.typed.expression_type(expression) != Some(descriptor.result_type())
            || !matches!(self.typed.types().get(descriptor.result_type()),
                Some(TypeKind::Nominal { nominal, .. }) if *nominal == case.root())
            || self.expression_ssa_type(expression, span)? != tagged
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let plan = self
            .owned
            .construction_plan(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.construction() != expression
            || plan.target() != descriptor.target()
            || !plan.deliveries().is_empty()
            || plan.terminating_operand().is_some()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_construction_root(descriptor.result_type(), descriptor.target(), plan, span)?;
        let (variant, _) = self
            .enum_payloads
            .get(&(tagged, case_id))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let integer = self.ssa_builtin(BuiltinType::Int, span)?;
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let (_, tag) = self.append(
            Operation::TaggedDiscriminant { owner },
            vec![EntityType::Value(integer)],
            span,
        )?;
        let (_, expected) = self.append(
            Operation::Constant(ScalarConstant::Integer(variant as i128)),
            vec![EntityType::Value(integer)],
            span,
        )?;
        let (_, result) = self.append(
            Operation::Compare {
                operator: ComparisonOperator::Equal,
                left: value(tag[0]),
                right: value(expected[0]),
            },
            vec![EntityType::Value(boolean)],
            span,
        )?;
        Ok(value(result[0]))
    }
}
