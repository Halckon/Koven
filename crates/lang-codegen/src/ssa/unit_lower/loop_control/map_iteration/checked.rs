//! Resume an active provider from the ordinary scalar carrier, proving every incoming alias.
use super::*;
impl UnitExpressionLowerer<'_> {
    pub(in super::super::super) fn resume_checked_map(
        &mut self,
        from: BlockId,
        before_borrows: &std::collections::BTreeMap<UnitSymbolId, LoanId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(context) = self.loops.last() else {
            return Ok(());
        };
        let Some(iteration) = &context.iteration else {
            return Ok(());
        };
        let IterationState::Map(state) = &iteration.state else {
            return Ok(());
        };
        let state = state.path;
        if state.conditional.is_some() {
            if state.carrier.nested.is_some() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            return self.resume_conditional_map_checked(state, from, before_borrows, span);
        }
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if state.carrier.active_body() != from
            || state.carrier.nested.is_some()
            || !matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let previous = state.carrier.current_entities(self.function, from, span)?;
        let current = nested::ParentRoles {
            block: self.block,
            caller: state.carrier.identity.current(self, span)?,
            source: self.map_pending_source(state.source_slot, span)?,
            cursor: self.map_pending_cursor(state.cursor_slot, state.cursor_type, span)?,
            key: self.map_pending_source(state.key_slot, span)?,
            value: self.map_pending_source(state.value_slot, span)?,
            guard: None,
        };
        for (now, old) in [
            (current.caller, previous[0]),
            (EntityId::Loan(current.source), previous[1]),
            (EntityId::Value(current.cursor), previous[2]),
            (
                EntityId::Loan(current.key),
                EntityId::Loan(*before_borrows.get(&state.key_binding).ok_or_else(bad)?),
            ),
            (
                EntityId::Loan(current.value),
                EntityId::Loan(*before_borrows.get(&state.value_binding).ok_or_else(bad)?),
            ),
        ] {
            transport::from_anchor(self.function, now, old, span)?;
        }
        if self.borrow_bindings.get(&state.key_binding) != Some(&current.key)
            || self.borrow_bindings.get(&state.value_binding) != Some(&current.value)
        {
            return Err(bad());
        }
        let Some(IterationState::Map(actual)) = self
            .loops
            .last_mut()
            .and_then(|c| c.iteration.as_mut())
            .map(|i| &mut i.state)
        else {
            return Err(bad());
        };
        actual.carrier.resumed = Some(current);
        Ok(())
    }
}
