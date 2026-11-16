//! Actual borrow-binding consumer argument dispatch.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn binding_consumer_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.fresh_source_argument(call, argument, loan, span)?;
        #[cfg(feature = "d10-catalog-experiment")]
        self.entry_argument(call, argument, loan);
        if !self.body.binding_plans.has_consumer(call)
            && !self.body.aux_binding_plans.has_consumer(call)
            && !self.body.distinct_aux_plans.has_consumer(call)
            && !self.body.later_aux_plans.has_consumer(call)
            && !self.body.auxiliary_sequence_plans.has_consumer(call)
            && !self.body.interleaved_aux_plans.has_consumer(call)
            && !self.body.multi_binding_plans.has_consumer(call)
        {
            return Ok(());
        }
        let (Expression::Name, name_span) = self.node(argument)? else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        // Resolve this actual Name through existing frontend names, never its canonical owner root.
        let binding = self.reference(name_span)?;
        if self.borrowed.get(&binding).copied() != Some(loan) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.body.binding_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.aux_binding_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.distinct_aux_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.later_aux_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.auxiliary_sequence_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.interleaved_aux_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.multi_binding_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )
    }
}
