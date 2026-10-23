//! Bounded retirement, owner returns and direct Int reads for a real nullable while view.
use super::*;
use crate::ssa::model::{Definition, SsaTypeKind};

use lang_frontend::type_checking::{
    NominalKind, UnitAggregateProjectionDescriptor, UnitAggregateProjectionKind,
    UnitAggregateProjectionReceiver,
};

#[cfg(test)]
mod retirement_tests;

impl UnitExpressionLowerer<'_> {
    /// The true flag qualifies only an unchanged nullable owner for this return.
    /// The caller must still perform its original transfer_owned_expression once.
    pub(super) fn lower_while_owner_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(LoweredValue, bool), LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(use_fact) = self.typed.non_null_use(id) else {
            return self.lower(expression).map(|result| (result, false));
        };
        let root = use_fact.symbol();
        let Some(proof) = self.nullable_proofs.get(&root).copied() else {
            return self.lower(expression).map(|result| (result, false));
        };
        if proof.while_scope().is_none() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let declared = concrete(storage)?;
        if concrete(self.return_type)? != declared {
            return self.lower(expression).map(|result| (result, false));
        }
        let narrowed = concrete(use_fact.narrowed_type())?;
        let nullable = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        if !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(inner)) if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&proof.inner)
            || self.ssa_types.get(nullable.index())
                != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if self.borrow_bindings.contains_key(&root)
            || self.typed.copyability(declared) != Copyability::MoveOnly
            || self.typed.copyability(narrowed) != Copyability::MoveOnly
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
            || self.nullable_proofs.iter().any(|(&other, candidate)| {
                other != root && (candidate.loan == proof.loan || candidate.owner == proof.owner)
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut operand = expression;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(node.payload(), Expression::Name | Expression::Group { .. }) {
                return self.lower(expression).map(|result| (result, false));
            }
            let operand_id = UnitExpressionId::new(self.source_unit, operand);
            let fact = self
                .typed
                .non_null_use(operand_id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != operand_id
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != use_fact.narrowed_type()
                || self.typed.expression_type(operand_id) != Some(fact.narrowed_type())
                || self.typed.expression_category(operand_id) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        self.ensure_no_while_proof_carriers(span)?;
        self.end_nullable_proof(root, span)?;
        // Preserve Name/Group AfterExpression facts before the existing transfer.
        let result = self.lower(expression)?;
        if result != LoweredValue::Value(proof.owner)
            || self.bindings.get(&root) != Some(&result)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok((result, true))
    }

    pub(super) fn reject_nested_while_proof(&self, span: Span) -> Result<(), LoweringError> {
        if !self.nullable_proofs.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Entry qualification is minted from actual entry Shared(Boolean) parameters.
    /// The existing named access slots keep that root identity across CFG rebinding.
    pub(super) fn ensure_no_while_proof_carriers(&self, span: Span) -> Result<(), LoweringError> {
        if !self.has_only_empty_source_call_frame() || !self.pending_operands.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.ensure_while_proof_non_call_carriers(span)
    }

    pub(super) fn ensure_while_proof_non_call_carriers(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.function.borrow_return.is_some()
            || self.function.carrier_return.is_some()
            || !self.capture_loans.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.temporary_range_slots.is_empty()
            || !self.short_range_ends.is_empty()
            || self.current_receiver.is_some()
            || self.has_iteration_context()
            || self.borrow_bindings.iter().any(|(root, &loan)| {
                !self.entry_boolean_roots.contains(root)
                    || !matches!(self.function.entity(EntityId::Loan(loan)).map(|data| data.ty),
                        Some(EntityType::Loan { kind: LoanKind::Shared, target })
                            if matches!(self.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean)))
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    // A freshly opened ordinary call frame carries no value or loan yet.
    // Keep the frame intact; any prefix or other carrier still uses the strict gate.
    fn has_only_empty_source_call_frame(&self) -> bool {
        let [frame] = self.pending_call_frames.as_slice() else {
            return self.pending_call_frames.is_empty();
        };
        if !self.pending_operands.is_empty()
            || frame.pending_start != 0
            || frame.receiver
            || !frame.nullable_children.is_empty()
            || !frame.loan_arguments.is_empty()
            || !frame.created_loans.is_empty()
            || !frame.abi_slots.is_empty()
            || frame.field_replace_owner.is_some()
            || !frame.shared_field_roots.is_empty()
            || !frame.exclusive_root_owners.is_empty()
            || frame.call.source_unit() != self.source_unit
        {
            return false;
        }
        self.typed.call(frame.call).is_some_and(|descriptor| {
            descriptor.expression() == frame.call
                && descriptor.receiver().is_none()
                && descriptor.result_source()
                    == lang_frontend::type_checking::CallableResultSource::Owned
                && matches!(
                    descriptor.target(),
                    lang_frontend::type_checking::UnitCallTarget::Declaration(_)
                )
        })
    }

    /// Retirement alone may ignore an already evaluated, independent Int prefix.
    /// No call/CFG state changes here; entry and owner-return keep their empty gate.
    fn has_only_retirement_int_prefix(&self) -> bool {
        let [frame] = self.pending_call_frames.as_slice() else {
            return false;
        };
        if self.pending_operands.is_empty()
            || frame.pending_start != 0
            || frame.receiver
            || !frame.nullable_children.is_empty()
            || !frame.loan_arguments.is_empty()
            || !frame.created_loans.is_empty()
            || !frame.abi_slots.is_empty()
            || frame.field_replace_owner.is_some()
            || !frame.shared_field_roots.is_empty()
            || !frame.exclusive_root_owners.is_empty()
            || frame.call.source_unit() != self.source_unit
            || !self
                .parsed
                .ast()
                .expressions()
                .get(frame.call.expression())
                .is_ok_and(|node| matches!(node.payload(), Expression::Call { .. }))
        {
            return false;
        }
        let Some(descriptor) = self.typed.call(frame.call) else {
            return false;
        };
        if descriptor.expression() != frame.call
            || descriptor.receiver().is_some()
            || descriptor.result_source()
                != lang_frontend::type_checking::CallableResultSource::Owned
            || descriptor.aborts()
            || descriptor.prints_line()
            || !matches!(
                descriptor.target(),
                lang_frontend::type_checking::UnitCallTarget::Declaration(_)
            )
        {
            return false;
        }
        let Some(integer) = self.typed.types().builtin(BuiltinType::Int) else {
            return false;
        };
        let Some(&integer_type) = self.type_ids.get(&integer) else {
            return false;
        };
        if self.typed.copyability(integer) != Copyability::Copyable
            || self.ssa_types.get(integer_type.index())
                != Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
        {
            return false;
        }
        let Some(block) = self.function.block(self.block) else {
            return false;
        };
        self.pending_operands.iter().all(|&entity| {
            let EntityId::Value(_) = entity else {
                return false;
            };
            let Some(data) = self.function.entity(entity) else {
                return false;
            };
            if data.ty != EntityType::Value(integer_type) {
                return false;
            }
            // Value origin is immaterial to Copyable Int lifetime. Require its
            // actual current-block definition, not a compatible type or raw ID.
            match data.definition {
                Definition::BlockParameter {
                    block: defined,
                    index,
                } => defined == self.block && block.parameters.get(index) == Some(&entity),
                Definition::InstructionResult { instruction, index } => {
                    block.instructions.contains(&instruction)
                        && self
                            .function
                            .instruction(instruction)
                            .is_some_and(|defined| {
                                defined.block == self.block
                                    && defined.results.get(index) == Some(&entity)
                            })
                }
            }
        })
    }

    pub(super) fn end_nullable_proof(
        &mut self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = self.nullable_proofs.get(&root).copied() else {
            return Ok(());
        };
        self.validate_nullable_proof_retirement(root, span)?;
        self.function
            .append_instruction(
                self.block,
                Operation::BorrowEnd { loan: proof.loan },
                Vec::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.nullable_proofs.remove(&root);
        Ok(())
    }

    pub(super) fn validate_nullable_proof_retirement(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let proof = self
            .nullable_proofs
            .get(&root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if proof.when_scope().is_some() {
            return self.validate_nullable_when_retirement(root, span);
        }
        // All checks precede both BorrowEnd and state removal, in particular an owner Drop.
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        if proof.while_scope().is_some() && self.has_only_retirement_int_prefix() {
            self.ensure_while_proof_non_call_carriers(span)?;
        } else {
            self.ensure_no_while_proof_carriers(span)?;
        }
        if self
            .nullable_proofs
            .iter()
            .any(|(other, candidate)| *other != root && candidate.loan == proof.loan)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Called only after the original operand/local/child cleanup facts.
    pub(super) fn retire_while_proofs(
        &mut self,
        returning: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let scope = self.current_while_proof_scope();
        let roots = self
            .nullable_proofs
            .iter()
            .filter_map(|(&root, proof)| {
                proof
                    .while_scope()
                    .is_some_and(|proof_scope| returning || Some(proof_scope) == scope)
                    .then_some(root)
            })
            .collect::<Vec<_>>();
        for root in roots {
            self.end_nullable_proof(root, span)?;
        }
        Ok(())
    }

    pub(super) fn reject_live_while_proof(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.nullable_proofs.contains_key(&root) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(super) fn read_while_proof_int_field(
        &mut self,
        expression: ExpressionId,
        projection: UnitAggregateProjectionDescriptor,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        let Some(use_fact) = self.typed.non_null_use(receiver) else {
            return Ok(None);
        };
        let Some(proof) = self.nullable_proofs.get(&use_fact.symbol()).copied() else {
            return Ok(None);
        };
        if proof.when_scope().is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        if receiver.source_unit() != self.source_unit {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut operands = vec![receiver.expression()];
        let mut leaf = receiver.expression();
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(leaf)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    leaf = *expression;
                    operands.push(leaf);
                }
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&use_fact.symbol()) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let receiver_type = concrete(use_fact.narrowed_type())?;
        let declared = concrete(use_fact.declared_type())?;
        let id = UnitExpressionId::new(self.source_unit, expression);
        if projection.expression() != id
            || projection.kind() != UnitAggregateProjectionKind::Field
            || use_fact.expression() != receiver
            || self.typed.expression_type(receiver) != Some(use_fact.narrowed_type())
            || self.typed.expression_type(id) != Some(projection.ty())
            || self.typed.symbol_type(use_fact.symbol()) != Some(use_fact.declared_type())
            || self.type_ids.get(&receiver_type) != Some(&proof.inner)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != self.type_ids.get(&declared).copied().map(EntityType::Value)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if self.expression_builtin_type(expression, span)? != Some(BuiltinType::Int)
            || self.typed.copyability(concrete(projection.ty())?) != Copyability::Copyable
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
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
        let target = self.expression_ssa_type(expression, span)?;
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
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::HeapFieldRead {
                    receiver: proof.loan,
                    field,
                },
                vec![EntityType::Value(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for operand in operands.into_iter().rev() {
            self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
                self.source_unit,
                operand,
            )))?;
        }
        Ok(Some(LoweredValue::Value(require_value(results[0], span)?)))
    }
}
