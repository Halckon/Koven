//! Consume the public action order once; component/source IDs always come from current slots.
use super::*;
use lang_frontend::ownership_checking::UnitDropFact;

impl UnitExpressionLowerer<'_> {
    pub(super) fn map_state(
        &self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<MapState, LoweringError> {
        self.loops
            .iter()
            .rev()
            .filter_map(|context| context.iteration.as_ref())
            .find_map(|iteration| match &iteration.state {
                IterationState::Map(state) if iteration.statement == statement => Some(**state),
                _ => None,
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }
    pub(super) fn set_map_phase(
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
        point: UnitDropPoint,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(statement, span)?;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if !state.carrier.is_body_point(point) {
            return Err(bad());
        }
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
        self.validate_nested_component(state, symbol, loan, span)?;
        if self.borrow_bindings.remove(&symbol) != Some(loan) {
            return Err(bad());
        }
        self.iteration_end_loan(loan, span)?;
        self.set_map_phase(statement, phase, span)
    }
    pub(in super::super) fn map_finish_provider(
        &mut self,
        statement: UnitStatementId,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(statement, span)?;
        let ready = match state.phase {
            MapPhase::Body {
                key_live: false,
                value_live: false,
            } => state.carrier.is_body_point(point),
            MapPhase::Terminal {
                point: actual,
                finished: false,
                source_ended: false,
                owner_dropped: false,
            } => actual == point,
            _ => false,
        };
        if !ready
            || self.borrow_bindings.contains_key(&state.key_binding)
            || self.borrow_bindings.contains_key(&state.value_binding)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let block = state.terminal_block(point, span)?;
        self.validate_map_state_current(state, block, span)?;
        self.set_map_phase(
            statement,
            MapPhase::Terminal {
                point,
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
        point: UnitDropPoint,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(statement, span)?;
        if !matches!(state.phase, MapPhase::Terminal {
            point: actual, finished: true, source_ended: false, owner_dropped: false,
        } if actual == point)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let block = state.terminal_block(point, span)?;
        self.validate_map_state_current(state, block, span)?;
        self.iteration_end_loan(self.map_pending_source(state.source_slot, span)?, span)?;
        self.set_map_phase(
            statement,
            MapPhase::Terminal {
                point,
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
            .find_map(|iteration| match &iteration.state {
                IterationState::Map(state)
                    if fact.target() == state.carrier.identity.drop_target() =>
                {
                    Some((iteration.statement, **state))
                }
                _ => None,
            })
        else {
            return Ok(false);
        };
        let span = fact.value_origin();
        let point = fact.point();
        if state.carrier.drop_at(point) != Some(fact)
            || !matches!(state.phase, MapPhase::Terminal {
                point: actual, finished: true, source_ended: true, owner_dropped: false,
            } if actual == point)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let owner = self.map_terminal_owner(state, point, span)?;
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
            || last.map(|instruction| &instruction.origin) != Some(&Origin::Source(span))
            || state.carrier.identity.is_registered(self)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.set_map_phase(
            statement,
            MapPhase::Terminal {
                point,
                finished: true,
                source_ended: true,
                owner_dropped: true,
            },
            span,
        )?;
        Ok(true)
    }

    /// Called after the complete action list, before break truncation or Return.
    pub(in super::super) fn map_validate_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        for iteration in self
            .loops
            .iter()
            .filter_map(|context| context.iteration.as_ref())
        {
            let IterationState::Map(state) = &iteration.state else {
                continue;
            };
            if !state.carrier.is_body_point(point) && point != state.carrier.exhaustion_point {
                continue;
            }
            let span = self.statement_span(iteration.statement.statement())?;
            let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
            if self.borrow_bindings.contains_key(&state.key_binding)
                || self.borrow_bindings.contains_key(&state.value_binding)
            {
                return Err(bad());
            }
            match state.phase {
                MapPhase::Body {
                    key_live: false,
                    value_live: false,
                } if state.carrier.is_body_point(point) => {
                    // FinishProvider/EndSource are mandatory for terminal exits.
                    if state.terminal_block(point, span).is_ok() {
                        return Err(bad());
                    }
                    self.validate_map_state_current(**state, state.body_block(), span)?;
                }
                MapPhase::Terminal {
                    point: actual,
                    finished: true,
                    source_ended: true,
                    owner_dropped,
                } if actual == point => {
                    if self.block != state.terminal_block(point, span)?
                        || owner_dropped != state.carrier.drop_at(point).is_some()
                    {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
        self.validate_conditional_map_cleanup(point)
    }
}
