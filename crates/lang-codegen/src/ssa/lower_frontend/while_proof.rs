//! Bounded while-proof retirement, owner returns and direct Copyable payload reads.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::DropPoint,
    parser::Expression,
    source::Span,
    type_checking::{
        AggregateProjectionDescriptor, AggregateProjectionKind, AggregateProjectionReceiver,
        Copyability, ExpressionCategory,
    },
};

use super::{
    BuiltinType, EntityId, EntityType, ExpressionLowerer, LoweredValue, LoweringError,
    LoweringErrorKind, NominalKind, Operation, TypeKind, builtin_type, error, span_key, value,
};
use crate::ssa::model::SsaTypeKind;

impl ExpressionLowerer<'_> {
    /// Returning the original nullable storage does not extract or rewrap its inner.
    /// Qualify the whole transparent operand before ending the real while view.
    pub(super) fn lower_while_owner_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(use_fact) = self.typed.non_null_use(expression) else {
            return self.lower(expression);
        };
        let root = use_fact.symbol();
        let Some(proof) = self.non_null_bindings.get(&root).copied() else {
            return self.lower(expression);
        };
        if !proof.is_while_origin() {
            return self.lower(expression);
        }
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let declared = self.resolve_type(storage, span)?;
        if self.resolve_type(self.return_type, span)? != declared {
            return self.lower(expression);
        }
        let narrowed = self.resolve_type(use_fact.narrowed_type(), span)?;
        let nullable = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.validate_non_null_proof(root, proof, &self.bindings, span)?;
        if !matches!(self.typed.types().get(declared), Some(TypeKind::Nullable(inner)) if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&proof.inner)
            || self.ssa_types.get(nullable.index())
                != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if self.borrow_bindings.contains_key(&root)
            || self.function.borrow_return.is_some()
            || self.typed.copyability(declared) != Some(Copyability::MoveOnly)
            || self.typed.copyability(narrowed) != Some(Copyability::MoveOnly)
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
            || self.non_null_bindings.iter().any(|(&other, candidate)| {
                other != root && (candidate.loan == proof.loan || candidate.owner == proof.owner)
            })
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut operand = expression;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(node.payload(), Expression::Name | Expression::Group { .. }) {
                return self.lower(expression);
            }
            let fact = self
                .typed
                .non_null_use(operand)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != operand
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(operand) != Some(fact.narrowed_type())
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        self.ensure_no_unresolved_while_proof_carriers(span)?;
        self.end_non_null_proof(root, span)?;
        // Keep every original Name/Group AfterExpression fact and the caller's
        // ControlTransfer cleanup. Phase 3 already consumes this return operand.
        self.lower(expression)
    }

    /// These carriers may name a child or a forwarded alias (including pending None).
    /// Do not infer independence from types, absent view slots, or a removed binding.
    /// Provider element IDs remain in their context after some cleanup actions, so
    /// this first slice conservatively refuses those contexts rather than guessing.
    pub(super) fn ensure_no_unresolved_while_proof_carriers(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.pending_call_loans.is_empty()
            || !self.capture_loans.is_empty()
            || self.borrow_bindings.values().any(|&loan| {
                // A caller-provided Boolean loan cannot contain this owned handle
                // or a view born inside this function. Keep its identity and life.
                !self.is_entry_loan(loan)
                    || !matches!(
                        self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                        Some(EntityType::Loan { kind: super::LoanKind::Shared, target })
                            if matches!(self.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean))
                    )
            })
            || !self.result_source_loans.is_empty()
            || self.readonly_receiver.is_some()
            || self.loops.iter().any(|context| context.for_loop.is_some())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Invoked after authorized cleanup, never to end an unresolved child on its behalf.
    pub(super) fn retire_while_proofs(
        &mut self,
        returning: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut ending = Vec::new();
        for (&symbol, &proof) in &self.non_null_bindings {
            if !proof.is_while_origin() {
                continue;
            }
            self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
            let inherited = !returning
                && self.loops.last().is_some_and(|context| {
                    context.entry_views.get(&symbol).is_some_and(|entry| {
                        entry.origin == proof.origin && entry.inner == proof.inner
                    })
                });
            if !inherited {
                ending.push(symbol);
            }
        }
        if !ending.is_empty() {
            self.ensure_no_unresolved_while_proof_carriers(span)?;
        }
        for symbol in ending {
            self.end_non_null_proof(symbol, span)?;
        }
        Ok(())
    }

    /// Assignment must have retired its old proof through RHS facts. Retesting in
    /// nullable if must not overwrite a live while proof before reuse is supported.
    pub(super) fn reject_live_while_proof(
        &self,
        symbol: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .non_null_bindings
            .get(&symbol)
            .is_some_and(|proof| proof.is_while_origin())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(super) fn read_while_proof_int_field(
        &mut self,
        expression: ExpressionId,
        projection: AggregateProjectionDescriptor,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        let Some(proof) = self.non_null_proof_for_expression(receiver, span)? else {
            return Ok(None);
        };
        if !proof.is_while_origin() {
            return Ok(None);
        }
        let use_fact = self
            .typed
            .non_null_use(receiver)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut operands = vec![receiver];
        let mut leaf = receiver;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(leaf)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    leaf = *expression;
                    operands.push(leaf);
                }
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&use_fact.symbol()) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let receiver_type = self.resolve_type(use_fact.narrowed_type(), span)?;
        let declared = self.resolve_type(use_fact.declared_type(), span)?;
        let field_type = self.resolve_type(projection.ty(), span)?;
        if projection.expression() != expression
            || projection.kind() != AggregateProjectionKind::Field
            || use_fact.expression() != receiver
            || self.typed.expression_type(receiver) != Some(use_fact.narrowed_type())
            || self.typed.expression_type(expression) != Some(projection.ty())
            || self.type_ids.get(&receiver_type) != Some(&proof.inner)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != self.type_ids.get(&declared).copied().map(EntityType::Value)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if builtin_type(self.typed, field_type) != Some(BuiltinType::Int)
            || self.typed.copyability(field_type) != Some(super::Copyability::Copyable)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(receiver_type)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|entry| entry.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class || !arguments.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|field| *field == projection.field())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.expression_ssa_type(expression, span)?;
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = self.ssa_types.get(proof.inner.index())
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self.heap_payloads.get(&proof.inner) != Some(payload)
            || !matches!(self.ssa_types.get(payload.index()), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.get(field) == Some(&target))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let (_, result) = self.append(
            Operation::HeapFieldRead {
                receiver: proof.loan,
                field,
            },
            vec![EntityType::Value(target)],
            span,
        )?;
        // Reading consumes no owner or child loan. Replay only the bypassed Name/Group
        // cleanup after the read; lower() still owns this field expression's cleanup.
        for operand in operands.into_iter().rev() {
            self.emit_drops(DropPoint::AfterExpression(operand))?;
        }
        Ok(Some(LoweredValue::Value(value(result[0]))))
    }
}
