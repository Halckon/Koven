//! Bounded retirement for a real nullable `If` view.
use super::*;

impl UnitExpressionLowerer<'_> {
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
        if self.function.borrow_return.is_some()
            || self.function.carrier_return.is_some()
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
                !(self.entry_boolean_roots.contains(root)
                    && self.ssa_types.get(target.index())
                        == Some(&crate::ssa::model::SsaTypeKind::Boolean))
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
        self.ensure_no_while_proof_carriers(span)?;
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
