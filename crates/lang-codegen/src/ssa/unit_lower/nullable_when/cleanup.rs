//! View retirement follows real cleanup facts and leaves unrelated outer prefixes live.
use super::*;
use lang_frontend::ownership_checking::UnitLoanTarget;

impl UnitExpressionLowerer<'_> {
    pub(super) fn emit_nullable_when_operand_drops(
        &mut self,
        operands: &[UnitExpressionId],
    ) -> Result<(), LoweringError> {
        for operand in operands.iter().rev() {
            self.emit_drops(UnitDropPoint::AfterExpression(*operand))?;
        }
        Ok(())
    }

    pub(in super::super) fn validate_nullable_when_retirement(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let proof = self
            .nullable_proofs
            .get(&root)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if proof.when_scope().is_none() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let aliases_owner = |entity| entity == EntityId::Value(proof.owner);
        let aliases_view = |entity| entity == EntityId::Loan(proof.loan);
        if self
            .pending_operands
            .iter()
            .copied()
            .any(|entity| aliases_owner(entity) || aliases_view(entity))
            || self.current_receiver.is_some_and(|receiver| {
                aliases_owner(receiver.entity) || aliases_view(receiver.entity)
            })
            || self
                .borrow_bindings
                .values()
                .any(|loan| *loan == proof.loan)
            || self.capture_loans.values().any(|loan| *loan == proof.loan)
            || self
                .result_source_loans
                .values()
                .any(|loans| loans.contains(&proof.loan))
            || self.short_range_ends.contains_key(&proof.loan)
            || self.nullable_proofs.iter().any(|(other, candidate)| {
                *other != root && (candidate.loan == proof.loan || candidate.owner == proof.owner)
            })
            || self
                .bindings
                .iter()
                .any(|(other, value)| *other != root && *value == LoweredValue::Value(proof.owner))
            || self.pending_call_frames.iter().any(|frame| {
                frame
                    .nullable_children
                    .iter()
                    .any(|child| child.parent_origin == proof.origin)
                    || (self.pending_operands.len() > frame.pending_start
                        && frame.loan_arguments.iter().any(|(argument, _)| {
                            self.owned.loans().iter().any(|fact| {
                                fact.call() == frame.call
                                    && fact.argument() == *argument
                                    && matches!(fact.target(), UnitLoanTarget::Place(path)
                                        if path.root() == root)
                            })
                        }))
            })
        {
            // This checks dependency, not global carrier emptiness. An outer call
            // prefix with a different owner/loan survives normal and loop exits.
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(super) fn retire_nullable_when_scope(
        &mut self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let roots = self
            .nullable_proofs
            .iter()
            .filter_map(|(&root, proof)| {
                proof
                    .when_scope()
                    .is_some_and(|scope| scope.control == control)
                    .then_some(root)
            })
            .collect::<Vec<_>>();
        for &root in &roots {
            self.validate_nullable_when_retirement(root, span)?;
        }
        for root in roots {
            self.end_nullable_proof(root, span)?;
        }
        Ok(())
    }

    /// Called after exiting call-frame children and exact ControlTransfer drops.
    /// Birth depth distinguishes a leaving When from any retained outer scope.
    pub(in super::super) fn retire_nullable_when_transfers(
        &mut self,
        minimum_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let roots = self
            .nullable_proofs
            .iter()
            .filter_map(|(&root, proof)| {
                proof
                    .when_scope()
                    .is_some_and(|scope| scope.loop_depth >= minimum_loop_depth)
                    .then_some(root)
            })
            .collect::<Vec<_>>();
        for &root in &roots {
            self.validate_nullable_when_retirement(root, span)?;
        }
        for root in roots {
            self.end_nullable_proof(root, span)?;
        }
        Ok(())
    }

    /// The producer's pure Name/Group Consume path only takes the named state.
    /// Check every original point, leaf first, before any SSA or owner mutation.
    /// Empty drop facts also make closure validation and late-Map drop dispatch
    /// structurally empty; empty end facts make end_result_bindings a no-op.
    pub(in crate::ssa::unit_lower) fn preflight_nullable_take_drops(
        &self,
        operands: &[UnitExpressionId],
        span: Span,
    ) -> Result<(), LoweringError> {
        for operand in operands.iter().rev() {
            let point = UnitDropPoint::AfterExpression(*operand);
            if self.owned.drops().iter().any(|fact| fact.point() == point)
                || self
                    .owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|fact| fact.point() == point)
                || self.owned.iteration_cleanup_at(point).is_some()
                || self
                    .owned
                    .conditional_receiver_drops()
                    .iter()
                    .any(|fact| fact.point() == point)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }
}
