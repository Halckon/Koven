//! Bounded retirement and owner returns for a real nullable String `If` view.
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::ownership_checking::UnitDropTarget;

#[cfg(test)]
mod string_return_tests;

impl UnitExpressionLowerer<'_> {
    /// The true flag preserves nullable storage; false uses the normal result adapter.
    /// The caller must still perform its original transfer_owned_expression once.
    pub(super) fn lower_proven_owner_return(
        &mut self,
        returning: ExpressionId,
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
        let string_if = proof.if_scope().is_some()
            && self.ssa_types.get(proof.inner.index()) == Some(&SsaTypeKind::StringOwner);
        if !string_if {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let declared = concrete(storage)?;
        let narrowed = concrete(use_fact.narrowed_type())?;
        let target = concrete(self.return_type)?;
        let take_inner = target == narrowed;
        if target != declared && !take_inner {
            return self.lower(expression).map(|result| (result, false));
        }
        let nullable = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        if !matches!(self.typed.types().get(declared), Some(UnitTypeKind::Nullable(inner)) if *inner == narrowed)
            || self.type_ids.get(&narrowed) != Some(&proof.inner)
            || !(self.ssa_types.get(nullable.index())
                == Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
                || self.ssa_types.get(nullable.index())
                    == Some(&SsaTypeKind::InlineNullable {
                        inner: proof.inner,
                        niche: true,
                    }))
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
            || self.nullable_proofs.iter().any(|(&other, candidate)| {
                other != root && (candidate.loan == proof.loan || candidate.owner == proof.owner)
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut operand = expression;
        let mut operands = Vec::new();
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
            operands.push(operand_id);
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
        let returning = UnitExpressionId::new(self.source_unit, returning);
        if !self.can_return_from_nullable_string_if(returning, Some(expression), span)? {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owned_root = self.owned.binding_kind(root)
            == Some(lang_frontend::ownership_checking::OwnershipBindingKind::Owned)
            || (self.owned.binding_kind(root).is_none()
                && self.owned.formed_owned_locals().contains(&root));
        if !owned_root
            || self.bindings.iter().any(|(&other, value)| {
                other != root && *value == LoweredValue::Value(proof.owner)
            })
            || self.temporaries.values().any(|owner| *owner == proof.owner)
            || self.owned.drops().iter().any(|fact| {
                fact.point() == UnitDropPoint::ControlTransfer(returning)
                    && fact.target() == UnitDropTarget::Named(root)
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // A returned proof owner cannot still occupy a pending call slot.
        // Its pure Name/Group cleanup must be empty before Take or BorrowEnd.
        self.ensure_string_if_proof_carriers(span)?;
        self.preflight_nullable_take_drops(&operands, span)?;
        let (returned_owner, returned_type) = if take_inner {
            self.validate_nullable_proof_retirement(root, span)?;
            let (_, values) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::NullableTake {
                        owner: proof.owner,
                        proof: proof.loan,
                    },
                    vec![EntityType::Value(proof.inner)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let inner = require_value(values[0], span)?;
            self.nullable_proofs.remove(&root);
            // Keep only the delivered inner binding until the existing transfer removes it.
            self.bindings.insert(root, LoweredValue::Value(inner));
            (inner, proof.inner)
        } else {
            self.end_nullable_proof(root, span)?;
            (proof.owner, nullable)
        };
        // Preserve Name/Group AfterExpression facts before the existing transfer.
        let result = self.lower(expression)?;
        if result != LoweredValue::Value(returned_owner)
            || self.bindings.get(&root) != Some(&result)
            || self
                .function
                .entity(EntityId::Value(returned_owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(returned_type))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok((result, !take_inner))
    }

    /// Entry qualification is minted from actual entry Shared(Boolean) parameters.
    /// The existing named access slots keep that root identity across CFG rebinding.
    pub(super) fn ensure_no_while_proof_carriers(&self, span: Span) -> Result<(), LoweringError> {
        if !self.pending_call_frames.is_empty() || !self.pending_operands.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.ensure_while_proof_non_call_carriers(span)
    }

    pub(super) fn ensure_while_proof_non_call_carriers(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.ensure_proof_non_call_carriers(false, span)
    }

    /// Only a direct String If view permits unrelated entry Shared(String) loans.
    /// Their root qualification survives the ordinary named-slot CFG rebinding.
    pub(super) fn ensure_string_if_proof_carriers(&self, span: Span) -> Result<(), LoweringError> {
        if !self.pending_call_frames.is_empty() || !self.pending_operands.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.ensure_string_if_proof_non_call_carriers(span)
    }

    pub(super) fn ensure_string_if_proof_non_call_carriers(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.ensure_proof_non_call_carriers(true, span)
    }

    pub(super) fn has_only_string_if_proofs(&self) -> bool {
        !self.nullable_proofs.is_empty()
            && self.nullable_proofs.values().all(|proof| {
                proof.if_scope().is_some()
                    && self.ssa_types.get(proof.inner.index()) == Some(&SsaTypeKind::StringOwner)
            })
    }

    fn ensure_proof_non_call_carriers(
        &self,
        allow_entry_strings: bool,
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
                let Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }) = self
                    .function
                    .entity(EntityId::Loan(loan))
                    .map(|data| data.ty)
                else {
                    return true;
                };
                !((self.entry_boolean_roots.contains(root)
                    && self.ssa_types.get(target.index()) == Some(&SsaTypeKind::Boolean))
                    || (allow_entry_strings
                        && self.entry_string_roots.contains(root)
                        && self.ssa_types.get(target.index()) == Some(&SsaTypeKind::StringOwner)))
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
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
        if self.has_only_string_if_proofs() {
            self.ensure_string_if_proof_carriers(span)?;
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
}
