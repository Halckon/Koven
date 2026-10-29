//! Text storage calls consume canonical frontend facts and reuse ordinary argument loans.
use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, PlaceAccess},
};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::CallArgument,
    source::Span,
    type_checking::{TextPrimitiveKind, UnitCallDescriptor, UnitExpressionId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_text_primitive(
        &mut self,
        expression: ExpressionId,
        arguments: &[CallArgument],
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let kind = descriptor
            .text_primitive()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let call = UnitExpressionId::new(self.source_unit, expression);
        let Some(lowered) = self.lower_call_arguments(call, arguments, descriptor, span)? else {
            return Ok(LoweredValue::Diverged);
        };
        let operation = match (kind, lowered.arguments.as_slice()) {
            (TextPrimitiveKind::Utf8ErrorOffset, [EntityId::Loan(source)]) => {
                Operation::Utf8ErrorOffset { source: *source }
            }
            (TextPrimitiveKind::ByteLength, [EntityId::Loan(source)]) => {
                Operation::StringByteLength { source: *source }
            }
            (TextPrimitiveKind::ByteAt, [EntityId::Loan(source), EntityId::Loan(index)]) => {
                let Some(EntityType::Loan { target, .. }) = self
                    .function
                    .entity(EntityId::Loan(*index))
                    .map(|entity| entity.ty)
                else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                let index = self.append_scalar(
                    Operation::Read {
                        source: PlaceAccess::Loan(*index),
                    },
                    target,
                    span,
                )?;
                let LoweredValue::Value(index) = index else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                Operation::StringByteAt {
                    source: *source,
                    index,
                }
            }
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, result) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let result = require_value(result[0], span)?;
        for (loan, end_span) in lowered.created_loans.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(end_span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
        }
        self.drop_abi_call_owners(lowered.abi_owners, span)?;
        self.emit_borrow_argument_expression_drops(call)?;
        self.emit_drops(UnitDropPoint::CallReturn(call))?;
        Ok(LoweredValue::Value(result))
    }
}
