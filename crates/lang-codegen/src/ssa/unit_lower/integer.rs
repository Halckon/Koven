//! Source-qualified integer intrinsics consume validated frontend facts.
use super::{LoweredValue, UnitExpressionLowerer, lowering_error};
use crate::ssa::{LoweringError, LoweringErrorKind, model::Operation};
use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{BuiltinType, IntegerOperationKind, UnitExpressionId, UnitTypeKind},
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
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.source_unit
            || self.typed.expression_type(descriptor.receiver()) != Some(descriptor.receiver_type())
            || self.typed.expression_type(id) != Some(descriptor.result_type())
            || !crate::ssa::integer::intrinsic_receiver_matches(
                self.parsed,
                expression,
                descriptor.receiver().expression(),
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let integer_types = [descriptor.receiver_type(), descriptor.result_type()]
            .into_iter()
            .all(|ty| {
                matches!(
                    self.typed.types().get(ty),
                    Some(UnitTypeKind::Builtin(
                        BuiltinType::Byte
                            | BuiltinType::Short
                            | BuiltinType::Int
                            | BuiltinType::Long
                            | BuiltinType::UByte
                            | BuiltinType::UShort
                            | BuiltinType::UInt
                            | BuiltinType::ULong
                    ))
                )
            });
        if !integer_types {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match descriptor.kind() {
            IntegerOperationKind::CheckedConvert => {
                // Phase 2 identity exists; checked conversion lowering is a separate delivery.
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            IntegerOperationKind::Invert
                if descriptor.receiver_type() != descriptor.result_type() =>
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            IntegerOperationKind::Invert => {}
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
