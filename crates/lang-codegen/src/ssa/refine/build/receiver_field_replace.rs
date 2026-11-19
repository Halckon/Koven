//! Refine one checked exchange while the current receiver stays borrowed.
use super::{super::receiver_field_replace::ReceiverField, Builder, Lowered, ROp};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, ScalarConstant, SsaTypeId},
};
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    parser::CallArgument,
    source::Span,
    type_checking::{
        ParameterMode, UnitAggregateProjectionDescriptor, UnitAggregateProjectionReceiver,
        UnitExpressionId, UnitTypeId,
    },
};

impl Builder<'_, '_> {
    pub(super) fn shared_current_heap_field(
        &mut self,
        projection: UnitAggregateProjectionDescriptor,
        span: Span,
    ) -> Result<Option<super::callable::ProjectionLoan>, LoweringError> {
        let Some(current) = self
            .inputs
            .receiver
            .filter(|current| current.mode == ParameterMode::Inout && self.is_class(current.ty))
        else {
            return Ok(None);
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        if receiver.source_unit() != self.inputs.source_unit {
            return Ok(None);
        }
        let mut root = receiver.expression();
        while let (lang_frontend::parser::Expression::Group { expression }, _) = self.node(root)? {
            root = expression;
        }
        let (payload, receiver_span) = self.node(root)?;
        if !matches!(payload, lang_frontend::parser::Expression::This)
            || self.reference(receiver_span).is_ok()
        {
            return Ok(None);
        }
        let base = self
            .receiver
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let field = *self
            .types
            .field_indices
            .get(&(current.ty, projection.field()))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.ssa_type(self.c(projection.ty()), span)?;
        let loan = self.value(
            ROp::SharedHeapFieldLoan { base, field },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        Ok(Some((loan, vec![loan], vec![receiver.expression()])))
    }

    pub(super) fn borrow_exclusive_heap_field(
        &mut self,
        base: super::Slot,
        owner_type: UnitTypeId,
        field: UnitSymbolId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<super::Slot, LoweringError> {
        let field = *self
            .types
            .field_indices
            .get(&(owner_type, field))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.value(
            ROp::ExclusiveHeapFieldLoan { base, field },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            },
            span,
        )
    }

    pub(super) fn exchange_receiver_field(
        &mut self,
        id: UnitExpressionId,
        field: ReceiverField,
        replacement: &CallArgument,
        value_ty: UnitTypeId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let base = self
            .receiver
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, field.begin))?;
        let loan = self.borrow_exclusive_heap_field(
            base,
            field.owner_type,
            field.field,
            target,
            field.begin,
        )?;
        let depth = self.pending_call_loans.len();
        self.pending_call_loans.push(loan);
        let is_unit = self.is_unit(value_ty);
        let value = match self.expression(replacement.value)? {
            Lowered::Diverged => return Ok(Lowered::Diverged),
            Lowered::Value(value) => value,
            Lowered::Unit if is_unit => self.value(
                ROp::Constant(ScalarConstant::Unit),
                EntityType::Value(target),
                replacement.span,
            )?,
            _ => return Err(error(LoweringErrorKind::MissingFact, replacement.span)),
        };
        let replacement_slot = self.adapt(value, replacement.value, value_ty)?;
        self.deliver(replacement.value);
        let replacement_slot = if self.move_only(value_ty) {
            replacement_slot
        } else {
            self.value(
                ROp::Copy {
                    source: replacement_slot,
                },
                EntityType::Value(target),
                replacement.span,
            )?
        };
        let loan = self
            .pending_call_loans
            .pop()
            .filter(|_| self.pending_call_loans.len() == depth)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, field.begin))?;
        let old = self.value(
            ROp::HeapReceiverFieldExchange {
                loan,
                replacement: replacement_slot,
            },
            EntityType::Value(target),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(if is_unit {
            Lowered::Unit
        } else {
            Lowered::Value(old)
        })
    }
}
