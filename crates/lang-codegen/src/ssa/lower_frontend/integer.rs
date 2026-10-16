//! Integer intrinsics consume frontend identity and type facts, never member spelling.
use super::{
    EntityType, ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, Operation,
    error, value,
};
use lang_frontend::{
    ast::ExpressionId,
    type_checking::{BuiltinType, IntegerOperationKind, TypeKind},
};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_integer_operation(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .integer_operation(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != expression
            || self.typed.expression_type(descriptor.receiver()) != Some(descriptor.receiver_type())
            || self.typed.expression_type(expression) != Some(descriptor.result_type())
            || !crate::ssa::integer::intrinsic_receiver_matches(
                self.parsed,
                expression,
                descriptor.receiver(),
            )
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let integer_types = [descriptor.receiver_type(), descriptor.result_type()]
            .into_iter()
            .all(|ty| {
                matches!(
                    self.typed.types().get(ty),
                    Some(TypeKind::Builtin(
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
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        match descriptor.kind() {
            IntegerOperationKind::Invert
                if descriptor.receiver_type() != descriptor.result_type() =>
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            IntegerOperationKind::Invert | IntegerOperationKind::CheckedConvert => {}
        }
        let kind = descriptor.kind();
        let operand = match self.lower(descriptor.receiver())? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        if kind == IntegerOperationKind::CheckedConvert {
            return self.checked_operation(Operation::CheckedIntegerConvert { operand }, ty, span);
        }
        let (_, results) = self.append(
            Operation::IntegerNot { operand },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }
}
