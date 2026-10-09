//! Copyable Map 查询的存在性、比较与受检提取。
use super::{LoweredValue, UnitExpressionLowerer, lowering_error, resolve_concrete_type};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{ComparisonOperator, EntityId, Operation, ScalarConstant, SsaTypeId, ValueId},
};
use lang_frontend::{
    ast::ExpressionId,
    parser::{BinaryOperator, Expression, LiteralKind},
    source::Span,
    type_checking::{BuiltinType, UnitExpressionId},
};

impl UnitExpressionLowerer<'_> {
    fn map_result_expression_type(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<SsaTypeId>, LoweringError> {
        let ty = self
            .typed
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
        Ok(self
            .type_ids
            .get(&ty)
            .copied()
            .filter(|ty| self.map_results.contains_key(ty)))
    }

    pub(super) fn is_map_result_value(&self, value: ValueId) -> bool {
        self.function
            .entity(EntityId::Value(value))
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
            .and_then(|data| self.map_results.get(&data.ty.semantic_type()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.append_scalar(Operation::MapResultUnwrap { result }, ty, span)
    }

    pub(super) fn wrap_map_result(
        &mut self,
        payload: ValueId,
        result: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let boolean = self
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .and_then(|ty| self.type_ids.get(&ty).copied())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let present = self.append_scalar(
            Operation::Constant(ScalarConstant::Boolean(true)),
            boolean,
            span,
        )?;
        let LoweredValue::Value(present) = present else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let wrapped = self.append_scalar(
            Operation::AggregateConstruct {
                aggregate: result,
                fields: vec![present, payload],
            },
            result,
            span,
        )?;
        let LoweredValue::Value(wrapped) = wrapped else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(wrapped)
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
            let aggregate = self.require_expression_value(nullable)?;
            let present = self.append_scalar(
                Operation::AggregateProject {
                    aggregate,
                    field: 0,
                },
                boolean,
                span,
            )?;
            if operator == BinaryOperator::NotEqual {
                return Ok(Some(present));
            }
            let LoweredValue::Value(operand) = present else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            return Ok(Some(self.append_scalar(
                Operation::BooleanNot { operand },
                boolean,
                span,
            )?));
        }
        let mut left = self.require_expression_value(left)?;
        let mut right = self.require_expression_value(right)?;
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
        let operator = if operator == BinaryOperator::Equal {
            ComparisonOperator::Equal
        } else {
            ComparisonOperator::NotEqual
        };
        Ok(Some(self.append_scalar(
            Operation::Compare {
                operator,
                left,
                right,
            },
            boolean,
            span,
        )?))
    }
}
