//! String If returns use original drops, after cancelling any qualified text call.
use super::*;

impl UnitExpressionLowerer<'_> {
    /// Qualify Return and any pending text call without emitting cleanup or SSA.
    pub(in crate::ssa::unit_lower) fn can_return_from_nullable_string_if(
        &self,
        expression: UnitExpressionId,
        value: Option<ExpressionId>,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if !self.has_only_string_if_proofs()
            || !self.loops.is_empty()
            || self.thunk_expression.is_some()
            || self
                .owned
                .iteration_cleanup_at(UnitDropPoint::ControlTransfer(expression))
                .is_some()
            || self.ensure_string_if_proof_non_call_carriers(span).is_err()
        {
            return Ok(false);
        }
        let returned = self
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if expression.source_unit() != self.source_unit
            || returned.span() != span
            || !matches!(returned.payload(), Expression::Return { value: actual, .. } if *actual == value)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // Owned return delivery while an If view is live remains a separate contract.
        if let Some(value) = value {
            let ty = self
                .typed
                .expression_type(UnitExpressionId::new(self.source_unit, value))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let concrete =
                resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
            if self.typed.copyability(concrete) != Copyability::Copyable {
                return Ok(false);
            }
        }
        if self.pending_call_frames.is_empty() {
            return Ok(self.pending_operands.is_empty());
        }
        let [receiver, arguments] = self.pending_call_frames.as_slice() else {
            return Ok(false);
        };
        if !receiver.receiver
            || arguments.receiver
            || receiver.call != arguments.call
            || receiver.loop_depth != 0
            || arguments.loop_depth != 0
            || receiver.pending_start != 0
            || arguments.pending_start != 1
            || !receiver.created_loans.is_empty()
            || !receiver.nullable_children.is_empty()
            || !receiver.loan_arguments.is_empty()
            || !receiver.abi_slots.is_empty()
            || !receiver.exclusive_root_owners.is_empty()
            || !receiver.shared_field_roots.is_empty()
            || receiver.field_replace_owner.is_some()
        {
            return Ok(false);
        }
        let call = receiver.call;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call {
            arguments: source_arguments,
            ..
        } = node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if !source_arguments.iter().any(|argument| {
            self.parsed
                .ast()
                .expressions()
                .get(argument.value)
                .is_ok_and(|node| {
                    let argument = node.span();
                    argument.source_id() == span.source_id()
                        && argument.start() <= span.start()
                        && span.end() <= argument.end()
                })
        }) {
            return Ok(false);
        }
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(loan) = self.nullable_string_receiver_loan(call, descriptor, node.span())? else {
            return Ok(false);
        };
        if self.pending_operands.first() != Some(&EntityId::Loan(loan)) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.supports_direct_control_prefix(call, source_arguments, descriptor, node.span())
    }
}
