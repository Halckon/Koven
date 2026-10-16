//! Lower only complete environment-bound numeric Range descriptors.
use super::{ExpressionLowerer, LoweredValue, error};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation},
};
use lang_frontend::{ast::ExpressionId, type_checking::TypeKind};
impl ExpressionLowerer<'_> {
    pub(super) fn lower_integer_range(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let id = expression;
        let descriptor = self
            .typed
            .integer_range_operation(id)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != id
            || self.typed.expression_type(descriptor.left()) != Some(descriptor.left_type())
            || self.typed.expression_type(descriptor.right()) != Some(descriptor.right_type())
            || self.typed.expression_type(id) != Some(descriptor.result_type())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let types = [
            descriptor.left_type(),
            descriptor.right_type(),
            descriptor.result_type(),
        ]
        .map(|ty| match self.typed.types().get(ty) {
            Some(TypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        });
        let left_id = descriptor.left();
        let right_id = descriptor.right();
        if !crate::ssa::integer_range::matches_fact(
            self.parsed,
            expression,
            left_id,
            right_id,
            descriptor.kind(),
            types,
        ) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let left = match self.lower(left_id)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let right = match self.lower(right_id)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (operation, negate) =
            crate::ssa::integer_range::operation(descriptor.kind(), left, right);
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        let EntityId::Value(operand) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if !negate {
            return Ok(LoweredValue::Value(operand));
        }
        let (_, results) = self.append(
            Operation::BooleanNot { operand },
            vec![EntityType::Value(ty)],
            span,
        )?;
        let EntityId::Value(result) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(LoweredValue::Value(result))
    }
}
