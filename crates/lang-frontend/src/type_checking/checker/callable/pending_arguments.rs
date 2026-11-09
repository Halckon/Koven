//! Ordinary call recovery follows source evaluation order, including prechecks.
use super::*;

impl Checker<'_> {
    pub(in crate::type_checking::checker) fn call_operand_recovery(
        &self,
        mut expression: ExpressionId,
        ty: TypeId,
    ) -> Option<CallRecovery> {
        let fallback = if self.is_error(ty) {
            CallRecovery::Rejected
        } else if self.is_deferred(ty) {
            CallRecovery::Pending
        } else {
            return None;
        };
        loop {
            if let Some(&recovery) = self.call_recoveries.get(&expression.index()) {
                return Some(recovery);
            }
            let Some(Expression::Group { expression: inner }) = self
                .ast()
                .expressions()
                .get(expression)
                .ok()
                .map(|node| node.payload())
            else {
                return Some(fallback);
            };
            expression = *inner;
        }
    }

    pub(super) fn pending_call_receiver_exit(
        &self,
        callee: ExpressionId,
    ) -> Result<bool, TypeCheckingError> {
        if let Expression::Member { receiver, .. } = self.ast().expressions().get(callee)?.payload()
            && !self.pending_checked_exit(*receiver)
        {
            return Ok(false);
        }
        Ok(self.pending_checked_exit(callee))
    }

    pub(in crate::type_checking::checker) fn note_deferred_argument_entry(
        &mut self,
        argument: ExpressionId,
    ) {
        let recovery = &mut self.iteration_recovery;
        recovery
            .deferred_argument_entries
            .entry(argument.index())
            .or_insert_with(|| recovery.bindings.clone());
    }

    pub(super) fn check_pending_untyped_arguments(
        &mut self,
        callee: ExpressionId,
        arguments: &[CallArgument],
    ) -> Result<(), TypeCheckingError> {
        let mut reachable = self.pending_call_receiver_exit(callee)?;
        for argument in arguments {
            self.check_pending_operand(argument.value, None, None, &mut reachable)?;
        }
        Ok(())
    }

    pub(super) fn check_deferred_arguments(
        &mut self,
        callee: ExpressionId,
        arguments: &[CallArgument],
    ) -> Result<(), TypeCheckingError> {
        self.check_expression(callee, None, None)?;
        self.check_pending_untyped_arguments(callee, arguments)
    }

    pub(super) fn note_pending_call_exit(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
    ) -> Result<(), TypeCheckingError> {
        let mut reachable = self.pending_call_receiver_exit(callee)?;
        // Safe-call arguments are conditional on a non-null receiver.
        if !matches!(
            self.ast().expressions().get(callee)?.payload(),
            Expression::Member { safe: true, .. }
        ) {
            for argument in arguments {
                reachable &= self.pending_checked_exit(argument.value);
            }
        }
        self.note_pending_exit(expression, reachable);
        Ok(())
    }

    pub(super) fn finish_unique_call(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        candidate: CallCandidate,
        mapping: Vec<usize>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let mut valid = true;
        let mut deferred = false;
        let mut rejected = false;
        let mut reaches_argument = self.pending_call_receiver_exit(callee)?;
        for (argument_index, argument) in arguments.iter().enumerate() {
            let parameter = &candidate.parameters[mapping[argument_index]];
            let was_checked = self.expression_types[argument.value.index()].is_some();
            let result = self.check_pending_operand(
                argument.value,
                Some(parameter.ty),
                parameter.span,
                &mut reaches_argument,
            )?;
            valid &= !self.is_error(result.ty);
            deferred |= self.is_deferred(result.ty);
            rejected |= self.call_operand_recovery(argument.value, result.ty)
                == Some(CallRecovery::Rejected);
            if (was_checked || parameter.mode == ParameterMode::Inout)
                && !self.is_error(result.ty)
                && !self.is_deferred(result.ty)
                && !self.call_parameter_matches(argument.value, result.ty, parameter)
            {
                self.mismatch(
                    self.ast().expressions().get(argument.value)?.span(),
                    parameter.span,
                    result.ty,
                    parameter.ty,
                )?;
                valid = false;
                rejected = true;
            }
            if !deferred
                && matches!(argument.mode_marker, Some(ParameterModeMarker::Inout(_)))
                && (self.expression_categories[argument.value.index()] != ExpressionCategory::Place
                    || self.is_mutable_element_place(argument.value) == Some(false))
            {
                self.emit_mapping_error(MappingError::Mode {
                    primary: argument
                        .mode_marker
                        .map(parameter_mode_span)
                        .unwrap_or(argument.span),
                    parameter: parameter.span,
                })?;
                valid = false;
                rejected = true;
            }
        }
        if deferred || !valid {
            let recovery = if rejected {
                CallRecovery::Rejected
            } else {
                CallRecovery::Pending
            };
            self.call_recoveries.insert(expression.index(), recovery);
            return Ok(ExprCheck {
                ty: if deferred {
                    self.deferred(DeferredReason::Call)
                } else {
                    self.error_type()
                },
                falls_through: true,
            });
        }
        self.record_call(expression, callee, arguments, candidate, mapping)
    }
}
