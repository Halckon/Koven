//! Exact statement and argument boundaries for the independent binding certificate.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn statement(&mut self, statement: StatementId) -> Result<Lowered, LoweringError> {
        let id = UnitStatementId::new(self.inputs.source_unit, statement);
        if self.body.binding_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .binding_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        let result = self.statement_inner(statement)?;
        let point = UnitDropPoint::AfterStatement(id);
        if result != Lowered::Diverged {
            self.drops(point)?;
        } else {
            self.unreachable_drop_points.push(point);
        }
        if self.body.binding_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.binding_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        Ok(result)
    }
    fn binding_quiescent(&self, span: Span) -> Result<(), LoweringError> {
        if !self.borrowed.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.pending_call_loans.is_empty()
            || !self.range_views.is_empty()
            || !self.short_ranges.is_empty()
            || !self.capture_loans.is_empty()
            || !self.pending_views.is_empty()
            || !self.active_views.is_empty()
            || !self.loops.is_empty()
            || !self.temporaries.is_empty()
            || !self.narrowed.is_empty()
            || self.receiver.is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn binding_consumer_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.body.binding_plans.has_consumer(call) {
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
        self.body
            .binding_plans
            .consumer_argument(call, self.unit(argument), binding, loan, span)
    }
}
