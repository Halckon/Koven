//! Source-qualified integer intrinsics consume validated frontend facts.
use super::{LoweredValue, UnitExpressionLowerer, lowering_error};
use crate::ssa::{LoweringError, LoweringErrorKind, model::Operation};
use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{IntegerOperationKind, UnitExpressionId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_integer_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .integer_operation(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.kind() != IntegerOperationKind::Invert
            || descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.source_unit
            || self.typed.expression_type(descriptor.receiver()) != Some(descriptor.receiver_type())
            || self.typed.expression_type(id) != Some(descriptor.result_type())
            || descriptor.receiver_type() != descriptor.result_type()
            || !crate::ssa::integer::intrinsic_receiver_matches(
                self.parsed,
                expression,
                descriptor.receiver().expression(),
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let operand = match self.lower(descriptor.receiver().expression())? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        self.append_scalar(Operation::IntegerNot { operand }, ty, span)
    }
}
