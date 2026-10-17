//! Consume the public action order once; component/source IDs always come from current slots.
use super::*;
use lang_frontend::ownership_checking::{UnitDropFact, UnitDropTarget};

impl UnitExpressionLowerer<'_> {
    fn map_state(&self, statement: UnitStatementId, span: Span) -> Result<MapState, LoweringError> {
        self.loops
            .iter()
            .rev()
            .filter_map(|context| context.iteration.as_ref())
            .find_map(|iteration| match iteration.state {
                IterationState::Map(state) if iteration.statement == statement => Some(state),
                _ => None,
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }
    fn set_map_phase(
        &mut self,
        statement: UnitStatementId,
        phase: MapPhase,
        span: Span,
    ) -> Result<(), LoweringError> {
        let iteration = self
            .loops
            .iter_mut()
            .rev()
            .filter_map(|context| context.iteration.as_mut())
            .find(|iteration| iteration.statement == statement)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let IterationState::Map(state) = &mut iteration.state else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        state.phase = phase;
        Ok(())
    }
    pub(in super::super) fn map_end_binding(
        &mut self,
        statement: UnitStatementId,
        symbol: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(statement, span)?;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let MapPhase::Body {
            key_live,
            value_live,
        } = state.phase
        else {
            return Err(bad());
        };
        let (slot, phase) = if symbol == state.value_binding && key_live && value_live {
            (
                state.value_slot,
                MapPhase::Body {
                    key_live: true,
                    value_live: false,
                },
            )
        } else if symbol == state.key_binding && key_live && !value_live {
            (
                state.key_slot,
                MapPhase::Body {
                    key_live: false,
                    value_live: false,
                },
            )
        } else {
            return Err(bad());
        };
        let loan = self.map_pending_source(slot, span)?;
        if self.borrow_bindings.remove(&symbol) != Some(loan) {
            return Err(bad());
        }
        self.iteration_end_loan(loan, span)?;
        self.set_map_phase(statement, phase, span)
    }
    pub(in super::super) fn map_finish_provider(
        &mut self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(statement, span)?;
        if !matches!(
            state.phase,
            MapPhase::Exhaustion {
                finished: false,
                source_ended: false,
                owner_dropped: false
            }
        ) || self.borrow_bindings.contains_key(&state.key_binding)
            || self.borrow_bindings.contains_key(&state.value_binding)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        state.carrier.validate_current(
            self,
            state.source_slot,
            state.cursor_slot,
            state.carrier.exhaustion,
            span,
        )?;
        self.set_map_phase(
            statement,
            MapPhase::Exhaustion {
                finished: true,
                source_ended: false,
                owner_dropped: false,
            },
            span,
        )
    }
    pub(in super::super) fn map_end_source(
        &mut self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(statement, span)?;
        if !matches!(
            state.phase,
            MapPhase::Exhaustion {
                finished: true,
                source_ended: false,
                owner_dropped: false
            }
        ) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        state.carrier.validate_current(
            self,
            state.source_slot,
            state.cursor_slot,
            state.carrier.exhaustion,
            span,
        )?;
        self.iteration_end_loan(self.map_pending_source(state.source_slot, span)?, span)?;
        self.set_map_phase(
            statement,
            MapPhase::Exhaustion {
                finished: true,
                source_ended: true,
                owner_dropped: false,
            },
            span,
        )
    }
    pub(in super::super) fn emit_map_source_drop(
        &mut self,
        fact: UnitDropFact,
    ) -> Result<bool, LoweringError> {
        let Some((statement, state)) = self
            .loops
            .iter()
            .rev()
            .filter_map(|context| context.iteration.as_ref())
            .find_map(|iteration| match iteration.state {
                IterationState::Map(state)
                    if fact.target() == UnitDropTarget::Named(state.carrier.root) =>
                {
                    Some((iteration.statement, state))
                }
                _ => None,
            })
        else {
            return Ok(false);
        };
        let span = fact.value_origin();
        if state.carrier.drop != Some(fact)
            || !matches!(
                state.phase,
                MapPhase::Exhaustion {
                    finished: true,
                    source_ended: true,
                    owner_dropped: false
                }
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let owner = state.carrier.exhaustion_owner(self, span)?;
        let source = self.map_pending_source(state.source_slot, span)?;
        let last = self
            .function
            .block(self.block)
            .and_then(|block| block.instructions.last())
            .and_then(|id| self.function.instruction(*id));
        if last.map(|instruction| &instruction.operation)
            != Some(&Operation::BorrowEnd { loan: source })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.emit_drop_fact(fact)?;
        let last = self
            .function
            .block(self.block)
            .and_then(|block| block.instructions.last())
            .and_then(|id| self.function.instruction(*id));
        if last.map(|instruction| &instruction.operation) != Some(&Operation::Drop { owner })
            || self.bindings.contains_key(&state.carrier.root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.set_map_phase(
            statement,
            MapPhase::Exhaustion {
                finished: true,
                source_ended: true,
                owner_dropped: true,
            },
            span,
        )?;
        Ok(true)
    }
    pub(super) fn map_validate_exhaustion(
        &self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(statement, span)?;
        let MapPhase::Exhaustion {
            finished: true,
            source_ended: true,
            owner_dropped,
        } = state.phase
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if owner_dropped != state.carrier.drop.is_some()
            || self.block != state.carrier.exhaustion
            || self.borrow_bindings.contains_key(&state.key_binding)
            || self.borrow_bindings.contains_key(&state.value_binding)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
}
