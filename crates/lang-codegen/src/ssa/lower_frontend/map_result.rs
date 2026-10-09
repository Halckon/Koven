//! Copyable Map 查询的存在性、比较与受检提取，不复用 pointer nullable 的 loan proof。
use super::{ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error, value};
use crate::ssa::model::{
    ComparisonOperator, EntityId, EntityType, Operation, ScalarConstant, SsaTypeId, ValueId,
};
use lang_frontend::{
    ast::ExpressionId,
    parser::{BinaryOperator, Expression, LiteralKind},
    source::Span,
};

impl ExpressionLowerer<'_> {
    pub(super) fn map_result_expression_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<SsaTypeId>, LoweringError> {
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ty = self.resolve_type(ty, span)?;
        Ok(self
            .type_ids
            .get(&ty)
            .copied()
            .filter(|ty| self.map_results.contains_key(ty)))
    }

    pub(super) fn is_map_result_value(&self, source: ValueId) -> bool {
        self.function
            .entity(EntityId::Value(source))
            .is_some_and(|data| self.map_results.contains_key(&data.ty.semantic_type()))
    }

    pub(super) fn unwrap_map_result(
        &mut self,
        result: ValueId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let ty = self
            .function
            .entity(EntityId::Value(result))
            .and_then(|data| self.map_results.get(&data.ty.semantic_type()).copied())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, output) = self.append(
            Operation::MapResultUnwrap { result },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(output[0])))
    }

    pub(super) fn wrap_map_result(
        &mut self,
        payload: ValueId,
        result: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let present = self
            .typed
            .types()
            .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
            .and_then(|ty| self.type_ids.get(&ty).copied())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, flag) = self.append(
            Operation::Constant(ScalarConstant::Boolean(true)),
            vec![EntityType::Value(present)],
            span,
        )?;
        let (_, wrapped) = self.append(
            Operation::AggregateConstruct {
                aggregate: result,
                fields: vec![value(flag[0]), payload],
            },
            vec![EntityType::Value(result)],
            span,
        )?;
        Ok(value(wrapped[0]))
    }

    fn is_null_literal(&self, expression: ExpressionId) -> bool {
        match self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map(|node| node.payload())
        {
            Ok(Expression::Literal(LiteralKind::Null)) => true,
            Ok(Expression::Group { expression }) => self.is_null_literal(*expression),
            _ => false,
        }
    }

    pub(super) fn lower_map_result_binary(
        &mut self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
            return Ok(None);
        }
        let left_type = self.map_result_expression_type(left, span)?;
        let right_type = self.map_result_expression_type(right, span)?;
        if left_type.is_none() && right_type.is_none() {
            return Ok(None);
        }
        let boolean = self.expression_ssa_type(expression, span)?;
        let nullable = if self.is_null_literal(right) && left_type.is_some() {
            Some(left)
        } else if self.is_null_literal(left) && right_type.is_some() {
            Some(right)
        } else {
            None
        };
        if let Some(nullable) = nullable {
            let aggregate = self.require_value(nullable)?;
            let (_, present) = self.append(
                Operation::AggregateProject {
                    aggregate,
                    field: 0,
                },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            let present = value(present[0]);
            if operator == BinaryOperator::NotEqual {
                return Ok(Some(LoweredValue::Value(present)));
            }
            let (_, absent) = self.append(
                Operation::BooleanNot { operand: present },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            return Ok(Some(LoweredValue::Value(value(absent[0]))));
        }
        let mut left = self.require_value(left)?;
        let mut right = self.require_value(right)?;
        if let Some(ty) = left_type
            && right_type.is_none()
        {
            right = self.wrap_map_result(right, ty, span)?;
        }
        if let Some(ty) = right_type
            && left_type.is_none()
        {
            left = self.wrap_map_result(left, ty, span)?;
        }
        let comparison = if operator == BinaryOperator::Equal {
            ComparisonOperator::Equal
        } else {
            ComparisonOperator::NotEqual
        };
        let (_, result) = self.append(
            Operation::Compare {
                operator: comparison,
                left,
                right,
            },
            vec![EntityType::Value(boolean)],
            span,
        )?;
        Ok(Some(LoweredValue::Value(value(result[0]))))
    }
}
