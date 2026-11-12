//! Actual boundary and Name lookup hooks for the independent child certificate.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn child_entry(&mut self, id: UnitStatementId) -> Result<(), LoweringError> {
        if !self.body.child_plans.has_entry(id) {
            return Ok(());
        }
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
            .child_plans
            .enter(id, &self.bindings, block, self.inputs.span)
    }
    pub(super) fn child_completion(
        &mut self,
        id: UnitStatementId,
        result: Lowered,
    ) -> Result<(), LoweringError> {
        if !self.body.child_plans.has_completion(id) {
            return Ok(());
        }
        self.binding_quiescent(self.inputs.span)?;
        if result == Lowered::Diverged {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        let block = self.current(self.inputs.span)?;
        self.body.child_plans.complete(
            id,
            &self.bindings,
            &self.dead,
            &self.consumed_drops,
            block,
            self.inputs.span,
        )
    }
    pub(super) fn child_initializer(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        parent: Option<UnitSymbolId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.body.child_plans.has_child(symbol) {
            return Ok(());
        }
        let (Expression::Name, name_span) = self.node(initializer)? else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let raw = self.reference(name_span)?;
        if parent != Some(raw)
            || self.borrowed.contains_key(&symbol)
            || self.result_source_loans.contains_key(&symbol)
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let slot = *self
            .borrowed
            .get(&raw)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.body
            .child_plans
            .child_activate(symbol, self.unit(initializer), raw, slot, span)
    }
    pub(super) fn child_consumer_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.body.child_plans.has_consumer(call) {
            return Ok(());
        }
        let (Expression::Name, name_span) = self.node(argument)? else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let binding = self.reference(name_span)?;
        if self.borrowed.get(&binding).copied() != Some(loan) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.body
            .child_plans
            .consumer_argument(call, self.unit(argument), binding, loan, span)
    }
}
