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

    pub(super) fn numeric_for(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        source: ExpressionId,
        loop_body: lang_frontend::ast::StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let statement_id =
            lang_frontend::type_checking::UnitStatementId::new(self.inputs.source_unit, statement);
        let plan = self
            .inputs
            .owned
            .iteration(statement_id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = plan.descriptor();
        let endpoint = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_type(boolean, span)?;
        let Some(range) = self.required(source)? else {
            return Ok(Lowered::Diverged);
        };
        let first = self.value(
            ROp::AggregateProject {
                aggregate: range,
                field: 0,
            },
            EntityType::Value(endpoint),
            span,
        )?;
        let last = self.value(
            ROp::AggregateProject {
                aggregate: range,
                field: 1,
            },
            EntityType::Value(endpoint),
            span,
        )?;
        let empty = self.value(
            ROp::AggregateProject {
                aggregate: range,
                field: 2,
            },
            EntityType::Value(boolean),
            span,
        )?;
        let current = self.value(
            ROp::Alias { source: first },
            EntityType::Value(endpoint),
            span,
        )?;
        let header = self.body.block();
        let continuation = self.body.block();
        let increment = self.body.block();
        let exhausted = self.body.block();
        let exit = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: empty,
                when_true: exhausted,
                when_false: header,
            },
            span,
        )?;
        let mark = self.enter(header);
        if let lang_frontend::type_checking::UnitSequentialIterationBinding::Name(symbol) =
            descriptor.binding()
        {
            let item = self.value(
                ROp::Alias { source: current },
                EntityType::Value(endpoint),
                span,
            )?;
            self.bindings.insert(*symbol, item);
        }
        self.loops.push(super::LoopFrame {
            statement: statement_id,
            continue_target: continuation,
            exit,
            views: mark,
            broken: false,
            seq_for: None,
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        self.loops.pop();
        self.leave(mark, span)?;
        if reached {
            self.goto(continuation, span)?;
        }
        let mark = self.enter(continuation);
        self.leave_unclaimed(mark, span)?;
        let terminal = self.value(
            ROp::Compare {
                operator: crate::ssa::model::ComparisonOperator::Equal,
                left: current,
                right: last,
            },
            EntityType::Value(boolean),
            span,
        )?;
        self.terminate(
            RTerm::Branch {
                condition: terminal,
                when_true: exhausted,
                when_false: increment,
            },
            span,
        )?;
        let mark = self.enter(increment);
        self.leave_unclaimed(mark, span)?;
        let one = self.value(
            ROp::Constant(ScalarConstant::Integer(1)),
            EntityType::Value(endpoint),
            span,
        )?;
        let next = self.value(
            ROp::Binary {
                operator: crate::ssa::model::BinaryOperator::Add,
                left: current,
                right: one,
            },
            EntityType::Value(endpoint),
            span,
        )?;
        self.push(ROp::Alias { source: next }, vec![current], span)?;
        self.goto(header, span)?;

        let facts = self.collect_drops(UnitDropPoint::LoopExit(statement_id));
        let mark = self.enter(exhausted);
        self.leave_unclaimed(mark, span)?;
        self.emit_drops(&facts)?;
        self.goto(exit, span)?;
        self.finish_drops(&facts);
        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        Ok(Lowered::Unit)
    }

    pub(super) fn constant_materialization(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        let id = self.unit(expression);
        let Some(facts) = self.inputs.typed.constants() else {
            return Ok(None);
        };
        let Ok(index) = facts
            .uses()
            .binary_search_by_key(&id, |use_| use_.expression())
        else {
            return Ok(None);
        };
        let descriptor = &facts.uses()[index];
        let plan = self
            .inputs
            .constant_owned
            .and_then(|owned| owned.materialization_at(id))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != descriptor {
            return Err(error(LoweringErrorKind::MismatchedAnalysis, span));
        }
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let op = match (plan.kind(), descriptor.value()) {
            (
                lang_frontend::ownership_checking::ConstantMaterializationKind::InlineCopy,
                lang_frontend::type_checking::ConstValue::Boolean(value),
            ) => ROp::Constant(ScalarConstant::Boolean(*value)),
            (
                lang_frontend::ownership_checking::ConstantMaterializationKind::InlineCopy,
                lang_frontend::type_checking::ConstValue::Integer { value, .. },
            ) => ROp::Constant(ScalarConstant::Integer(*value)),
            (
                lang_frontend::ownership_checking::ConstantMaterializationKind::StringTemporary,
                lang_frontend::type_checking::ConstValue::String(bytes),
            ) => ROp::StringLiteral {
                string: ty,
                bytes: bytes.to_vec(),
            },
            _ => return Err(error(LoweringErrorKind::MismatchedAnalysis, span)),
        };
        self.value(op, EntityType::Value(ty), span)
            .map(Lowered::Value)
            .map(Some)
    }
}
