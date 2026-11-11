//! Validated String clone / field-loan lowering and Copyable Map result comparison/unwrap helpers.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::DropPoint,
    parser::BinaryOperator,
    source::Span,
    type_checking::{ParameterMode, StringOperationKind},
};

use super::{
    ComparisonOperator, EntityId, EntityType, ExpressionLowerer, LoanKind, LoweredValue,
    LoweringError, LoweringErrorKind, Operation, ScalarConstant, SsaTypeId, ValueId, error, value,
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

    pub(super) fn is_null_literal(&self, expression: ExpressionId) -> bool {
        crate::ssa::lowering_support::is_null_literal(self.parsed, expression)
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

    pub(super) fn lower_string_clone(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .string_operation(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let effect = self
            .owned
            .string_effect(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.kind() != StringOperationKind::Clone
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || descriptor.result_mode() != ParameterMode::Value
            || effect.receiver() != descriptor.receiver()
            || effect.receiver_type() != descriptor.receiver_type()
            || effect.result_type() != descriptor.result_type()
            || effect.kind() != descriptor.kind()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver();
        let receiver_span = self.expression_span(receiver)?;
        let (source, created) = if self.clone_field_projection(receiver)?.is_some() {
            let mut created = Vec::new();
            let source = self.clone_field_loan(receiver, &mut created)?;
            (source, created)
        } else {
            let (source, created) =
                self.lower_borrow_argument(expression, receiver, receiver_span)?;
            (source, if created { vec![source] } else { Vec::new() })
        };
        let result_type = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::StringClone { source },
            vec![EntityType::Value(result_type)],
            span,
        )?;
        for loan in created.into_iter().rev() {
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.emit_drops(DropPoint::AfterExpression(receiver))?;
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn clone_field_projection(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<lang_frontend::type_checking::AggregateProjectionDescriptor>, LoweringError>
    {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
            return self.clone_field_projection(*expression);
        }
        Ok(self.typed.aggregate_projection(expression))
    }

    pub(super) fn clone_field_loan(
        &mut self,
        expression: ExpressionId,
        created: &mut Vec<super::LoanId>,
    ) -> Result<super::LoanId, LoweringError> {
        use lang_frontend::type_checking::{AggregateProjectionReceiver, NominalKind, TypeKind};
        let span = self.expression_span(expression)?;
        let target = self.expression_ssa_type(expression, span)?;
        if let Some((loan, loans)) = self.readonly_view(expression)? {
            created.extend(loans);
            return Ok(loan);
        }

        if let Some(projection) = self.clone_field_projection(expression)? {
            let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let receiver_type = self
                .typed
                .expression_type(receiver)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let receiver_type = self.resolve_type(receiver_type, span)?;
            let Some(TypeKind::Nominal { nominal, .. }) = self.typed.types().get(receiver_type)
            else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let nominal = self
                .typed
                .nominals()
                .iter()
                .find(|value| value.id() == *nominal)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let field = nominal
                .fields()
                .iter()
                .position(|field| *field == projection.field())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let kind = nominal.kind();
            let base = self.clone_field_loan(receiver, created)?;
            let operation = match kind {
                NominalKind::Class => Operation::SharedHeapFieldLoan { base, field },
                NominalKind::ValueClass => Operation::SharedFieldLoan { base, field },
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let (_, results) = self.append(
                operation,
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                span,
            )?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            created.push(loan);
            return Ok(loan);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
            return self.clone_field_loan(*expression, created);
        }
        if matches!(node.payload(), lang_frontend::parser::Expression::Name)
            && let Some(symbol) = self.references.get(&super::span_key(node.span()))
            && let Some(loan) = self.borrow_bindings.get(symbol).copied()
        {
            return Ok(loan);
        }
        if self.references.get(&super::span_key(node.span())).is_some_and(|symbol| {
            self.owned.loans().iter().any(|active| {
                self.pending_call_loans.contains_key(&(active.call().index(), active.argument().index()))
                    && active.kind() == lang_frontend::ownership_checking::LoanKind::Exclusive
                    && matches!(active.target(), lang_frontend::ownership_checking::LoanTarget::Place(path)
                        if !path.is_root() && path.root() == *symbol)
            })
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.require_value(expression)?;
        let (_, root) = self.append(
            Operation::RootPlace { owner },
            vec![EntityType::Place(target)],
            span,
        )?;
        let (_, result) = self.append(
            Operation::BorrowBegin {
                place: super::place(root[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            span,
        )?;
        let EntityId::Loan(loan) = result[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        created.push(loan);
        Ok(loan)
    }
}
