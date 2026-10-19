//! Resume an active provider from the ordinary scalar carrier, proving every incoming alias.
use super::*;
impl UnitExpressionLowerer<'_> {
    pub(in super::super::super) fn resume_checked_map(
        &mut self,
        from: BlockId,
        before_borrows: &std::collections::BTreeMap<UnitSymbolId, LoanId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let states = self
            .loops
            .iter()
            .filter_map(|c| c.iteration.as_ref())
            .filter_map(|i| match &i.state {
                IterationState::Map(s) => Some((i.statement, s.path)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (statement, state) in states {
            if state.conditional.is_some() {
                self.resume_conditional_map_checked(state, from, before_borrows, span)?;
                continue;
            }
            self.resume_direct_map_checked(statement, state, from, before_borrows, span)?;
        }
        Ok(())
    }

    fn resume_direct_map_checked(
        &mut self,
        statement: UnitStatementId,
        state: MapState,
        from: BlockId,
        before_borrows: &std::collections::BTreeMap<UnitSymbolId, LoanId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if state.carrier.active_body() != from
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
        let current = state::ResumedRoles {
            block: self.block,
            caller: state.carrier.identity.current(self, span)?,
            source: self.map_pending_source(state.source_slot, span)?,
            cursor: self.map_pending_cursor(state.cursor_slot, state.cursor_type, span)?,
            key: self.map_pending_source(state.key_slot, span)?,
            value: self.map_pending_source(state.value_slot, span)?,
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
        self.conditional_runtime_mut(statement, span)?
            .carrier
            .resumed = Some(current);
        Ok(())
    }
}
