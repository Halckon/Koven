//! Consume validated String clone and call-loan facts, never the member spelling.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::DropPoint,
    type_checking::{ParameterMode, StringOperationKind},
};

use super::{
    EntityType, ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, Operation,
    error, value,
};

impl ExpressionLowerer<'_> {
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
}

impl ExpressionLowerer<'_> {
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

    fn clone_field_loan(
        &mut self,
        expression: ExpressionId,
        created: &mut Vec<super::LoanId>,
    ) -> Result<super::LoanId, LoweringError> {
        use super::{EntityId, LoanKind};
        use lang_frontend::type_checking::{AggregateProjectionReceiver, NominalKind, TypeKind};
        let span = self.expression_span(expression)?;
        let target = self.expression_ssa_type(expression, span)?;
        if let Some((loan, loans)) = self.deinit_view(expression)? {
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
