//! Integer intrinsics (`inv`, checked conversions), prefix `+`/`-`, `IntRange`/`LongRange`
//! value operations, `String.clone()`, and trusted text primitives in the refined CFG (SPEC-0371).
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{Expression, LiteralKind, PrefixOperator},
    source::Span,
    type_checking::{
        BuiltinType, IntegerOperationKind, IntegerRangeOperationKind, ParameterMode,
        StringOperationKind, TextPrimitiveKind,
    },
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{CheckedArithmeticOperator, EntityType, LoanKind, ScalarConstant},
    refine::body::{RAbort, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn prefix(
        &mut self,
        operator: PrefixOperator,
        operand: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if operator == PrefixOperator::Not {
            let Some(operand) = self.required(operand)? else {
                return Ok(Lowered::Diverged);
            };
            return Ok(Lowered::Value(
                self.typed_value(ROp::BooleanNot { operand }, expression)?,
            ));
        }
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        if operator == PrefixOperator::Minus {
            let (payload, operand_span) = self.node(operand)?;
            if let Expression::Literal(LiteralKind::Integer(kind)) = payload {
                let text = self
                    .inputs
                    .sources
                    .slice(operand_span)
                    .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
                let magnitude = lang_frontend::type_checking::integer_literal_magnitude(text, kind)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?;
                let value = if magnitude == (1u128 << 127) {
                    i128::MIN
                } else {
                    i128::try_from(magnitude)
                        .ok()
                        .and_then(i128::checked_neg)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?
                };
                return Ok(Lowered::Value(self.value(
                    ROp::Constant(ScalarConstant::Integer(value)),
                    EntityType::Value(ty),
                    span,
                )?));
            }
        }
        let Some(operand) = self.required(operand)? else {
            return Ok(Lowered::Diverged);
        };
        match operator {
            PrefixOperator::Plus => Ok(Lowered::Value(operand)),
            PrefixOperator::Minus => {
                let zero = self.value(
                    ROp::Constant(ScalarConstant::Integer(0)),
                    EntityType::Value(ty),
                    span,
                )?;
                let boolean = self
                    .inputs
                    .typed
                    .types()
                    .builtin(BuiltinType::Boolean)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let boolean = self.ssa_type(boolean, span)?;
                let result = self.body.slot(EntityType::Value(ty));
                let failed = self.body.slot(EntityType::Value(boolean));
                self.push(
                    ROp::CheckedArithmetic {
                        operator: CheckedArithmeticOperator::Subtract,
                        left: zero,
                        right: operand,
                    },
                    vec![result, failed],
                    span,
                )?;
                let abort = self.body.block();
                let success = self.body.block();
                self.terminate(
                    RTerm::Branch {
                        condition: failed,
                        when_true: abort,
                        when_false: success,
                    },
                    span,
                )?;
                self.block = Some(abort);
                self.terminate(
                    RTerm::Abort(RAbort::CheckedArithmetic {
                        operator: CheckedArithmeticOperator::Subtract,
                        divisor: operand,
                    }),
                    span,
                )?;
                self.block = Some(success);
                Ok(Lowered::Value(result))
            }
            PrefixOperator::Not => unreachable!("handled above"),
        }
    }

    pub(super) fn integer_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .integer_operation(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(operand) = self.required(descriptor.receiver().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        match descriptor.kind() {
            IntegerOperationKind::Invert => Ok(Lowered::Value(self.value(
                ROp::IntegerNot { operand },
                EntityType::Value(ty),
                span,
            )?)),
            IntegerOperationKind::CheckedConvert => {
                let boolean = self
                    .inputs
                    .typed
                    .types()
                    .builtin(BuiltinType::Boolean)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let boolean = self.ssa_type(boolean, span)?;
                let result = self.body.slot(EntityType::Value(ty));
                let failed = self.body.slot(EntityType::Value(boolean));
                self.push(
                    ROp::CheckedIntegerConvert { operand },
                    vec![result, failed],
                    span,
                )?;
                let abort = self.body.block();
                let success = self.body.block();
                self.terminate(
                    RTerm::Branch {
                        condition: failed,
                        when_true: abort,
                        when_false: success,
                    },
                    span,
                )?;
                self.block = Some(abort);
                self.terminate(RTerm::Abort(RAbort::CheckedConversion { target: ty }), span)?;
                self.block = Some(success);
                Ok(Lowered::Value(result))
            }
        }
    }

    pub(super) fn integer_range_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .integer_range_operation(id)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(left) = self.required(descriptor.left().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let Some(right) = self.required(descriptor.right().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let (op, negate) = match descriptor.kind() {
            IntegerRangeOperationKind::Inclusive | IntegerRangeOperationKind::Exclusive => (
                ROp::IntegerRangeConstruct {
                    first: left,
                    bound: right,
                    inclusive: descriptor.kind() == IntegerRangeOperationKind::Inclusive,
                },
                false,
            ),
            IntegerRangeOperationKind::Contains | IntegerRangeOperationKind::NotContains => (
                ROp::IntegerRangeContains {
                    value: left,
                    range: right,
                },
                descriptor.kind() == IntegerRangeOperationKind::NotContains,
            ),
            IntegerRangeOperationKind::Equal | IntegerRangeOperationKind::NotEqual => (
                ROp::IntegerRangeEqual { left, right },
                descriptor.kind() == IntegerRangeOperationKind::NotEqual,
            ),
        };
        let mut result = self.typed_value(op, expression)?;
        if negate {
            result = self.typed_value(ROp::BooleanNot { operand: result }, expression)?;
        }
        Ok(Lowered::Value(result))
    }

    pub(super) fn string_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .string_operation(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.kind() != StringOperationKind::Clone
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || descriptor.result_mode() != ParameterMode::Value
            || descriptor.receiver().source_unit() != self.inputs.source_unit
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver().expression();
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let (source, created_loan, view_read) =
            if let Some(loan) = self.borrowed_string_loan(receiver)? {
                (loan, None, Some(receiver))
            } else if let Some(view) = self.narrowed_view(receiver)? {
                (view, None, Some(receiver))
            } else {
                let Some(owner) = self.required(receiver)? else {
                    return Ok(Lowered::Diverged);
                };
                let receiver_span = self.span(receiver);
                let place = self.value(
                    ROp::RootPlace { owner },
                    EntityType::Place(ty),
                    receiver_span,
                )?;
                let loan = self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    },
                    receiver_span,
                )?;
                (loan, Some(loan), None)
            };
        let result = self.value(ROp::StringClone { source }, EntityType::Value(ty), span)?;
        if let Some(loan) = created_loan {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        if let Some(read) = view_read {
            self.drops(UnitDropPoint::AfterExpression(self.unit(read)))?;
        }
        Ok(Lowered::Value(result))
    }

    pub(super) fn text_primitive_call(
        &mut self,
        expression: ExpressionId,
        kind: TextPrimitiveKind,
        lowered: &[Slot],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let op = match (kind, lowered) {
            (TextPrimitiveKind::CopyBytes, [source]) => ROp::StringCopyBytes { source: *source },
            (TextPrimitiveKind::CopySlice, [source, start, end]) => {
                let start = self.text_index_slot(*start, span)?;
                let end = self.text_index_slot(*end, span)?;
                ROp::StringCopySlice {
                    source: *source,
                    start,
                    end,
                }
            }
            (TextPrimitiveKind::Utf8ErrorOffset, [source]) => {
                ROp::Utf8ErrorOffset { source: *source }
            }
            (TextPrimitiveKind::ByteLength, [source]) => ROp::StringByteLength { source: *source },
            (TextPrimitiveKind::ByteAt, [source, index]) => {
                let index = self.text_index_slot(*index, span)?;
                ROp::StringByteAt {
                    source: *source,
                    index,
                }
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        Ok(Lowered::Value(self.typed_value(op, expression)?))
    }

    fn text_index_slot(&mut self, loan: Slot, span: Span) -> Result<Slot, LoweringError> {
        let EntityType::Loan { target, .. } = self.body.slots[loan.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.value(ROp::ReadLoan { loan }, EntityType::Value(target), span)
    }
}
