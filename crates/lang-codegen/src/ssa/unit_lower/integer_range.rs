//! Lower only complete environment-bound numeric Range descriptors.
use super::{LoweredValue, UnitExpressionLowerer, lowering_error};
use crate::ssa::{LoweringError, LoweringErrorKind, model::Operation};
use lang_frontend::source::Span;
use lang_frontend::{
    ast::ExpressionId,
    type_checking::{UnitExpressionId, UnitTypeKind},
};
impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_integer_range(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .integer_range_operation(id)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != id
            || self.typed.expression_type(descriptor.left()) != Some(descriptor.left_type())
            || self.typed.expression_type(descriptor.right()) != Some(descriptor.right_type())
            || self.typed.expression_type(id) != Some(descriptor.result_type())
            || descriptor.left().source_unit() != self.source_unit
            || descriptor.right().source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let types = [
            descriptor.left_type(),
            descriptor.right_type(),
            descriptor.result_type(),
        ]
        .map(|ty| match self.typed.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        });
        let left_id = descriptor.left().expression();
        let right_id = descriptor.right().expression();
        if !crate::ssa::integer_range::matches_fact(
            self.parsed,
            expression,
            left_id,
            right_id,
            descriptor.kind(),
            types,
        ) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let left = match self.lower(left_id)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let right = match self.lower(right_id)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (operation, negate) =
            crate::ssa::integer_range::operation(descriptor.kind(), left, right);
        let result = self.append_scalar(operation, ty, span)?;
        let LoweredValue::Value(operand) = result else {
            return Ok(result);
        };
        if negate {
            self.append_scalar(Operation::BooleanNot { operand }, ty, span)
        } else {
            Ok(result)
        }
    }
}
