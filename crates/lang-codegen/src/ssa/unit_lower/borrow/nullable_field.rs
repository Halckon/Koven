//! Call-scoped String field children of an existing nullable while proof.
use lang_frontend::{
    ownership_checking::{UnitLoanFact, UnitLoanTarget},
    type_checking::{
        CallableResultSource, NominalKind, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver,
    },
};

use super::super::*;
use crate::ssa::model::SsaTypeKind;

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_while_proof_field_borrow(
        &mut self,
        argument: ExpressionId,
        fact: &UnitLoanFact,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let UnitLoanTarget::Place(path) = fact.target() else {
            return Ok(None);
        };
        let root = path.root();
        let Some(proof) = self.nullable_proofs.get(&root).copied() else {
            return Ok(None);
        };
        if path.is_root() {
            return Ok(None);
        }
        let [field_symbol] = path.fields() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if path.element().is_some() || self.borrow_bindings.contains_key(&root) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let call = self
            .typed
            .call(fact.call())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if call.result_source() != CallableResultSource::Owned {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut operands = vec![argument];
        let mut expression = argument;
        while let Expression::Group { expression: inner } = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            expression = *inner;
            operands.push(expression);
        }
        let id = UnitExpressionId::new(self.source_unit, expression);
        let projection = self
            .typed
            .aggregate_projection(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let UnitAggregateProjectionReceiver::Expression(mut receiver) = projection.receiver()
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let use_fact = self
            .typed
            .non_null_use(receiver)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let declared = concrete(storage)?;
        let narrowed = concrete(use_fact.narrowed_type())?;
        if fact.argument() != UnitExpressionId::new(self.source_unit, argument)
            || fact.call().source_unit() != self.source_unit
            || receiver.source_unit() != self.source_unit
            || projection.expression() != id
            || projection.kind() != UnitAggregateProjectionKind::Field
            || projection.field() != *field_symbol
            || operands.iter().any(|&operand| {
                self.typed
                    .expression_type(UnitExpressionId::new(self.source_unit, operand))
                    != Some(projection.ty())
            })
            || self.type_ids.get(&concrete(projection.ty())?) != Some(&target)
            || !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(inner)) if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&proof.inner)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != self.type_ids.get(&declared).copied().map(EntityType::Value)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if builtin_type(self.typed, concrete(projection.ty())?) != Some(BuiltinType::String)
            || self.ssa_types.get(target.index()) != Some(&SsaTypeKind::StringOwner)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        loop {
            let fact = self
                .typed
                .non_null_use(receiver)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != receiver
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(receiver) != Some(fact.narrowed_type())
                || self.typed.expression_category(receiver) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(receiver.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    receiver = UnitExpressionId::new(self.source_unit, *expression);
                }
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(narrowed)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = self
            .field_indices
            .get(&(narrowed, *field_symbol))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal
            .fields()
            .get(field)
            .is_none_or(|field| field.symbol() != *field_symbol || field.ty() != projection.ty())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = self.ssa_types.get(proof.inner.index())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.heap_payloads.get(&proof.inner) != Some(payload)
            || !matches!(self.ssa_types.get(payload.index()), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.get(field) == Some(&target))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let frame = self
            .pending_call_frames
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if frame.call != fact.call() || frame.receiver {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // The actual call frame is checked before emission. Only the child is
        // pending; the current proof remains owned by its original while scope.
        let (_, result) = self
            .function
            .append_instruction(
                self.block,
                Operation::SharedHeapFieldLoan {
                    base: proof.loan,
                    field,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Loan(loan)] = result.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let frame = self
            .pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        frame.shared_field_roots.push(root);
        // Only ordinary source calls get transport evidence; legacy immediate
        // print/function-value children keep their original cleanup path.
        if call.receiver().is_none()
            && !call.prints_line()
            && !call.aborts()
            && matches!(
                call.target(),
                lang_frontend::type_checking::UnitCallTarget::Declaration(_)
            )
        {
            frame
                .nullable_children
                .push(call_lifetimes::nullable_child::PendingNullableChild {
                    call: fact.call(),
                    argument: fact.argument(),
                    root,
                    parent_origin: proof.origin,
                    parent_inner: proof.inner,
                    field,
                    target,
                    slot: self.pending_operands.len(),
                });
        }
        Ok(Some(*loan))
    }
}
