//! Source-qualified String clone consumes the frontend's intrinsic and ownership facts.
use super::{LoweredValue, UnitExpressionLowerer, lowering_error};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{Operation, Origin},
};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::{ParameterMode, StringOperationKind, UnitExpressionId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_string_clone(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .string_operation(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let effect = self
            .owned
            .string_effect(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.kind() != StringOperationKind::Clone
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || descriptor.result_mode() != ParameterMode::Value
            || effect.receiver() != descriptor.receiver()
            || effect.receiver_type() != descriptor.receiver_type()
            || effect.result_type() != descriptor.result_type()
            || effect.kind() != descriptor.kind()
            || descriptor.receiver().source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver().expression();
        let receiver_span = self
            .parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let ty = self.expression_ssa_type(expression, span)?;
        let (source, created) = if self.clone_field_projection(receiver)?.is_some()
            && !self.is_this_field_path(receiver, receiver_span)?
        {
            let mut created = Vec::new();
            let source = self.clone_field_loan(receiver, &mut created, span)?;
            (source, created)
        } else {
            let (source, created, _) =
                self.lower_borrow_argument(id, receiver, ty, receiver_span, span)?;
            (source, created)
        };
        let result = self.append_scalar(Operation::StringClone { source }, ty, span)?;
        for loan in created.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(result)
    }
}

impl UnitExpressionLowerer<'_> {
    fn clone_field_projection(
        &self,
        expression: ExpressionId,
    ) -> Result<
        Option<lang_frontend::type_checking::UnitAggregateProjectionDescriptor>,
        LoweringError,
    > {
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
        Ok(self
            .typed
            .aggregate_projection(UnitExpressionId::new(self.source_unit, expression)))
    }

    fn clone_field_loan(
        &mut self,
        expression: ExpressionId,
        created: &mut Vec<super::LoanId>,
        span: Span,
    ) -> Result<super::LoanId, LoweringError> {
        use super::{EntityId, EntityType, LoanKind};
        use lang_frontend::type_checking::{
            NominalKind, UnitAggregateProjectionReceiver, UnitTypeKind,
        };
        let target = self.expression_ssa_type(expression, span)?;
        if let Some(projection) = self.clone_field_projection(expression)? {
            let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
            else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            if receiver.source_unit() != self.source_unit {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let receiver_type = self
                .typed
                .expression_type(receiver)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let receiver_type = super::resolve_concrete_type(
                self.typed,
                receiver_type,
                self.substitutions,
                self.static_self,
                span,
            )?;
            let Some(UnitTypeKind::Nominal { declaration, .. }) =
                self.typed.types().get(receiver_type)
            else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            let nominal = self
                .typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let field = self
                .field_indices
                .get(&(receiver_type, projection.field()))
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let kind = nominal.kind();
            let base = self.clone_field_loan(receiver.expression(), created, span)?;
            let operation = match kind {
                NominalKind::Class => Operation::SharedHeapFieldLoan { base, field },
                NominalKind::ValueClass => Operation::SharedFieldLoan { base, field },
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    operation,
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    }],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            created.push(loan);
            return Ok(loan);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
            return self.clone_field_loan(*expression, created, span);
        }
        if matches!(node.payload(), lang_frontend::parser::Expression::Name)
            && let Some(symbol) = self.references.get(&super::span_key(node.span()))
            && let Some(loan) = self.borrow_bindings.get(symbol).copied()
        {
            return Ok(loan);
        }
        let owner = self.require_expression_value(expression)?;
        if self.pending_call_frames.iter().any(|frame| {
            frame.field_replace_owner.is_some_and(|slot| {
                self.pending_operands.get(slot) == Some(&EntityId::Value(owner))
            })
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (_, root) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = root[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        created.push(loan);
        Ok(loan)
    }
}
