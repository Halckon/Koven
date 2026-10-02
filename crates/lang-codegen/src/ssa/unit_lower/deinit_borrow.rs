//! Readonly current-receiver field loans keep all parent loans live through a call.

use super::*;
use lang_frontend::{
    ownership_checking::UnitLoanTarget,
    type_checking::{NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver},
};

struct ReceiverFieldLoan {
    symbol: UnitSymbolId,
    field: usize,
    target: SsaTypeId,
}

struct ReceiverFieldPath {
    base: LoanId,
    fields: Vec<ReceiverFieldLoan>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn is_this_field_path(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        Ok(self.this_field_path(expression, span)?.is_some())
    }

    pub(super) fn lower_this_field_borrow(
        &mut self,
        expression: ExpressionId,
        loan_target: &UnitLoanTarget,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<(LoanId, Vec<LoanId>)>, LoweringError> {
        let Some(path) = self.this_field_path(expression, span)? else {
            return Ok(None);
        };
        let UnitLoanTarget::Place(place) = loan_target else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(first) = path.fields.first() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if place.element().is_some()
            || place.root() != first.symbol
            || !place.fields().iter().copied().eq(path
                .fields
                .iter()
                .skip(1)
                .map(|field| field.symbol))
            || path.fields.last().map(|field| field.target) != Some(target)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut base = path.base;
        let mut created = Vec::new();
        for field in path.fields {
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::SharedHeapFieldLoan {
                        base,
                        field: field.field,
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: field.target,
                    }],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            created.push(loan);
            base = loan;
        }
        Ok(Some((base, created)))
    }

    fn this_field_path(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<ReceiverFieldPath>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.this_field_path(*expression, span);
        }
        if let Some(field) = self.current_receiver_field(expression, span)? {
            let aggregate::CurrentReceiverFieldStorage::Heap {
                receiver,
                receiver_kind: LoanKind::Shared,
            } = field.storage
            else {
                return Ok(None);
            };
            return Ok(Some(ReceiverFieldPath {
                base: receiver,
                fields: vec![ReceiverFieldLoan {
                    symbol: field.symbol,
                    field: field.field,
                    target: field.ssa_type,
                }],
            }));
        }
        let Some(projection) = self
            .typed
            .aggregate_projection(UnitExpressionId::new(self.source_unit, expression))
        else {
            return Ok(None);
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || receiver.source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(mut path) = self.this_field_path(receiver.expression(), span)? else {
            return Ok(None);
        };
        let receiver_type = self
            .typed
            .expression_type(receiver)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let receiver_type = resolve_concrete_type(
            self.typed,
            receiver_type,
            self.substitutions,
            self.static_self,
            span,
        )?;
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(receiver_type)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class || !arguments.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = self
            .field_indices
            .get(&(receiver_type, projection.field()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let field_type = nominal
            .fields()
            .get(field)
            .map(|field| field.ty())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if projection.ty() != field_type {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let target = self
            .type_ids
            .get(&field_type)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        path.fields.push(ReceiverFieldLoan {
            symbol: projection.field(),
            field,
            target,
        });
        Ok(Some(path))
    }
}
