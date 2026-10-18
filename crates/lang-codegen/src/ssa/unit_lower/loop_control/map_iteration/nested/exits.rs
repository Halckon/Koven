//! Cumulative point witnesses are never restored with a path-local frame checkpoint.
mod returned;
use super::*;
use lang_frontend::ownership_checking::{UnitDropTarget, UnitIterationCleanupAction as Action};

#[derive(Clone, Copy)]
pub(super) struct Event {
    pub(super) point: UnitDropPoint,
    pub(super) block: BlockId,
    caller: EntityId,
    source: LoanId,
    cursor: ValueId,
    key: Option<LoanId>,
    value: Option<LoanId>,
    pub(super) guard: Option<(UnitSymbolId, ValueId)>,
    start: usize,
    pub(super) end: Option<usize>,
    returned: Option<ValueId>,
}

#[derive(Clone, Copy, Default)]
pub(in crate::ssa::unit_lower) struct NestedState {
    pub(super) guard_birth: Option<ValueId>,
    events: [Option<Event>; 3],
}

impl NestedState {
    pub(super) fn event(self, point: UnitDropPoint) -> Option<Event> {
        self.events
            .into_iter()
            .flatten()
            .find(|event| event.point == point)
    }
}

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa::unit_lower) fn nested_state_mut(
        &mut self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<&mut NestedState, LoweringError> {
        self.loops
            .iter_mut()
            .filter_map(|context| context.iteration.as_mut())
            .find_map(|iteration| match &mut iteration.state {
                IterationState::Map(state) if iteration.statement == statement => {
                    Some(&mut state.nested)
                }
                _ => None,
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    pub(in crate::ssa::unit_lower) fn begin_nested_map_cleanup(
        &mut self,
        point: UnitDropPoint,
        actions: &[Action],
    ) -> Result<(), LoweringError> {
        let members = self
            .loops
            .iter()
            .filter_map(|context| context.iteration.as_ref())
            .filter_map(|iteration| match &iteration.state {
                IterationState::Map(state)
                    if state.carrier.nested.and_then(|pair| pair.tail).is_some()
                        && (state.carrier.is_body_point(point)
                            || state.carrier.exhaustion_point == point) =>
                {
                    Some((iteration.statement, **state))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            return Ok(());
        }
        for (statement, state) in members {
            let span = self.statement_span(statement.statement())?;
            let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
            let pair = state.carrier.nested.ok_or_else(bad)?;
            let tail = pair.tail.ok_or_else(bad)?;
            let exhausted = state.carrier.exhaustion_point == point;
            if self.owned.iteration_cleanup_at(point) != Some(actions)
                || state.nested.event(point).is_some()
            {
                return Err(bad());
            }
            let plan = self.owned.map_iteration(statement).ok_or_else(bad)?;
            let exits = plan
                .exits()
                .iter()
                .filter(|exit| exit.point() == point)
                .collect::<Vec<_>>();
            if !matches!(exits.as_slice(), [exit] if exit.actions() == actions) {
                return Err(bad());
            }
            if exhausted {
                if !matches!(state.phase, MapPhase::Terminal { point: actual, finished: false, source_ended: false, owner_dropped: false } if actual == point)
                {
                    return Err(bad());
                }
            } else if !matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            ) {
                return Err(bad());
            }
            self.validate_map_state_current(
                state,
                if exhausted {
                    state.carrier.exhaustion
                } else {
                    state.body_block()
                },
                span,
            )?;
            let mut anchor = state.nested.guard_birth;
            let guard = if exhausted {
                None
            } else {
                let guard = tail.guards[usize::from(statement == pair.inner)];
                let owner = self.nested_guard_current(guard, span)?;
                if anchor.is_none() {
                    anchor = Some(self.nested_guard_birth(guard, state.carrier.body, span)?);
                }
                transport::from_anchor(
                    self.function,
                    EntityId::Value(owner),
                    EntityId::Value(anchor.ok_or_else(bad)?),
                    span,
                )?;
                Some((guard.symbol, owner))
            };
            let event = Event {
                point,
                block: self.block,
                caller: state.carrier.identity.current(self, span)?,
                source: self.map_pending_source(state.source_slot, span)?,
                cursor: self.map_pending_cursor(state.cursor_slot, state.cursor_type, span)?,
                key: (!exhausted)
                    .then(|| self.map_pending_source(state.key_slot, span))
                    .transpose()?,
                value: (!exhausted)
                    .then(|| self.map_pending_source(state.value_slot, span))
                    .transpose()?,
                guard,
                start: self
                    .function
                    .block(self.block)
                    .ok_or_else(bad)?
                    .instructions
                    .len(),
                end: None,
                returned: None,
            };
            let evidence = self.nested_state_mut(statement, span)?;
            if exhausted && event.start != 0 {
                return Err(bad());
            }
            evidence.guard_birth = anchor;
            let slot = evidence
                .events
                .iter_mut()
                .find(|event| event.is_none())
                .ok_or_else(bad)?;
            *slot = Some(event);
        }
        Ok(())
    }

    pub(in crate::ssa::unit_lower) fn finish_nested_map_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let events = self
            .loops
            .iter()
            .filter_map(|context| context.iteration.as_ref())
            .filter_map(|iteration| match &iteration.state {
                IterationState::Map(state) => state
                    .nested
                    .event(point)
                    .map(|event| (iteration.statement, event)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let Some((statement, first)) = events.first().copied() else {
            return Ok(());
        };
        let span = self.statement_span(statement.statement())?;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let actions = self.owned.iteration_cleanup_at(point).ok_or_else(bad)?;
        let mut expected = Vec::new();
        for action in actions {
            let operation = match action {
                Action::Drop(fact) => {
                    let guards = events
                        .iter()
                        .filter_map(|(_, event)| event.guard)
                        .filter(|(symbol, _)| fact.target() == UnitDropTarget::Named(*symbol))
                        .collect::<Vec<_>>();
                    let [(_, owner)] = guards.as_slice() else {
                        return Err(bad());
                    };
                    Some((
                        Operation::Drop { owner: *owner },
                        Some(Origin::Source(fact.value_origin())),
                    ))
                }
                Action::EndBinding { statement, symbol } => {
                    let state = self.map_state(*statement, span)?;
                    let event = state.nested.event(point).ok_or_else(bad)?;
                    let loan = if *symbol == state.key_binding {
                        event.key
                    } else if *symbol == state.value_binding {
                        event.value
                    } else {
                        return Err(bad());
                    };
                    Some((
                        Operation::BorrowEnd {
                            loan: loan.ok_or_else(bad)?,
                        },
                        None,
                    ))
                }
                Action::FinishProvider(statement) => {
                    if self
                        .map_state(*statement, span)?
                        .nested
                        .event(point)
                        .is_none()
                    {
                        return Err(bad());
                    }
                    None
                }
                Action::EndSource(statement) => {
                    let event = self
                        .map_state(*statement, span)?
                        .nested
                        .event(point)
                        .ok_or_else(bad)?;
                    Some((Operation::BorrowEnd { loan: event.source }, None))
                }
                _ => return Err(bad()),
            };
            expected.extend(operation);
        }
        let block = self.function.block(self.block).ok_or_else(bad)?;
        if events.iter().any(|(_, event)| {
            event.block != self.block || event.start != first.start || event.end.is_some()
        }) || block.instructions.len() != first.start + expected.len()
        {
            return Err(bad());
        }
        for (id, (operation, origin)) in block.instructions[first.start..].iter().zip(expected) {
            let instruction = self.function.instruction(*id).ok_or_else(bad)?;
            if instruction.operation != operation
                || origin.is_some_and(|origin| instruction.origin != origin)
            {
                return Err(bad());
            }
        }
        // Check the whole block too: a duplicate consume before the recorded cleanup
        // boundary is not made safe by the correct suffix or by path-local phase bits.
        for (statement, event) in &events {
            let count = |operation: Operation| {
                block
                    .instructions
                    .iter()
                    .filter(|id| {
                        self.function
                            .instruction(**id)
                            .is_some_and(|instruction| instruction.operation == operation)
                    })
                    .count()
            };
            if let Some((_, owner)) = event.guard {
                if count(Operation::Drop { owner }) != 1 {
                    return Err(bad());
                }
            }
            for loan in [event.key, event.value].into_iter().flatten() {
                if count(Operation::BorrowEnd { loan }) != 1 {
                    return Err(bad());
                }
            }
            let terminal = actions
                .iter()
                .any(|action| *action == Action::EndSource(*statement));
            if count(Operation::BorrowEnd { loan: event.source }) != usize::from(terminal) {
                return Err(bad());
            }
            let EntityId::Loan(caller) = event.caller else {
                return Err(bad());
            };
            if count(Operation::BorrowEnd { loan: caller }) != 0 {
                return Err(bad());
            }
        }
        let end = block.instructions.len();
        for (statement, _) in events {
            let evidence = self.nested_state_mut(statement, span)?;
            let event = evidence
                .events
                .iter_mut()
                .flatten()
                .find(|event| event.point == point)
                .ok_or_else(bad)?;
            event.end = Some(end);
        }
        Ok(())
    }

    pub(super) fn nested_parent_return_completed(
        &self,
        state: MapState,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let Some(tail) = state.carrier.nested.and_then(|pair| pair.tail) else {
            return Ok(false);
        };
        let UnitIterationExitKind::Return(expression) = tail.kind else {
            return Ok(false);
        };
        let point = UnitDropPoint::ControlTransfer(expression);
        let Some(event) = state.nested.event(point) else {
            return Ok(false);
        };
        let valid = matches!(state.phase, MapPhase::Terminal { point: actual, finished: true, source_ended: true, owner_dropped: false } if actual == point)
            && event.end.is_some()
            && event.returned.is_some();
        if !valid {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(true)
    }

    pub(in crate::ssa::unit_lower) fn validate_nested_map_body(
        &mut self,
        context: &mut LoopContext,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(iteration) = context.iteration.as_mut() else {
            return Err(bad());
        };
        let IterationState::Map(state) = &mut iteration.state else {
            return Err(bad());
        };
        let Some(pair) = state.carrier.nested else {
            return Ok(());
        };
        let Some(tail) = pair.tail else { return Ok(()) };
        let body = state
            .nested
            .event(state.carrier.body_point)
            .ok_or_else(bad)?;
        if body.end.is_none() || body.block != self.block {
            return Err(bad());
        }
        if iteration.statement == pair.inner
            && matches!(tail.kind, UnitIterationExitKind::Return(_))
        {
            let result = self.validate_nested_return(pair, body, span)?;
            state
                .nested
                .events
                .iter_mut()
                .flatten()
                .find(|event| event.point == body.point)
                .ok_or_else(bad)?
                .returned = Some(result);
            self.nested_state_mut(pair.outer, span)?
                .events
                .iter_mut()
                .flatten()
                .find(|event| event.point == body.point)
                .ok_or_else(bad)?
                .returned = Some(result);
        }
        if iteration.statement == pair.outer
            || matches!(tail.kind, UnitIterationExitKind::Continue(_))
        {
            let block = self.function.block(body.block).ok_or_else(bad)?;
            let end = body.end.ok_or_else(bad)?;
            let next = block
                .instructions
                .get(end)
                .and_then(|id| self.function.instruction(*id))
                .ok_or_else(bad)?;
            let next_count = block
                .instructions
                .iter()
                .filter(|id| {
                    self.function.instruction(**id).is_some_and(|instruction| {
                        matches!(instruction.operation, Operation::MapIterationNext { .. })
                    })
                })
                .count();
            if block.instructions.len() != end + 1
                || next_count != 1
                || next.operation
                    != (Operation::MapIterationNext {
                        source: body.source,
                        cursor: body.cursor,
                    })
            {
                return Err(bad());
            }
        }
        if matches!(
            tail.kind,
            UnitIterationExitKind::Break(_) | UnitIterationExitKind::Return(_)
        ) && iteration.statement == pair.inner
        {
            let block = self.function.block(body.block).ok_or_else(bad)?;
            if block.instructions.len() != body.end.ok_or_else(bad)?
                || block.instructions.iter().any(|id| {
                    self.function.instruction(*id).is_some_and(|instruction| {
                        matches!(instruction.operation, Operation::MapIterationNext { .. })
                    })
                })
            {
                return Err(bad());
            }
        }
        Ok(())
    }

    pub(in crate::ssa::unit_lower) fn validate_nested_map_complete(
        &self,
        context: &LoopContext,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(iteration) = &context.iteration else {
            return Err(bad());
        };
        let IterationState::Map(state) = &iteration.state else {
            return Err(bad());
        };
        let Some(pair) = state.carrier.nested else {
            return Ok(());
        };
        let Some(tail) = pair.tail else { return Ok(()) };
        let expected = 2 + usize::from(
            iteration.statement == pair.outer
                && matches!(tail.kind, UnitIterationExitKind::Return(_)),
        );
        if state.nested.events.iter().flatten().count() != expected
            || state
                .nested
                .events
                .iter()
                .flatten()
                .any(|event| event.end.is_none())
            || state.nested.guard_birth.is_none()
        {
            return Err(bad());
        }
        let exhausted = state
            .nested
            .event(state.carrier.exhaustion_point)
            .ok_or_else(bad)?;
        let exhausted_block = self.function.block(exhausted.block).ok_or_else(bad)?;
        // At this boundary the false block has only its own terminal cleanup.
        // If it becomes the outer resume block, that later body's witness checks
        // the completed block has exactly its own outer Next and no inner Next.
        if exhausted_block.instructions.iter().any(|id| {
            self.function.instruction(*id).is_some_and(|instruction| {
                matches!(instruction.operation, Operation::MapIterationNext { .. })
            })
        }) {
            return Err(bad());
        }
        if exhausted.block != state.carrier.exhaustion
            || exhausted.guard.is_some()
            || exhausted.key.is_some()
            || exhausted.value.is_some()
            || exhausted.returned.is_some()
        {
            return Err(bad());
        }
        if let UnitIterationExitKind::Return(expression) = tail.kind {
            let event = state
                .nested
                .event(UnitDropPoint::ControlTransfer(expression))
                .ok_or_else(bad)?;
            let result = event.returned.ok_or_else(bad)?;
            let block = self.function.block(event.block).ok_or_else(bad)?;
            if !matches!(block.terminator.as_ref().map(|term| &term.kind), Some(TerminatorKind::Return { values }) if values.as_slice() == [result])
            {
                return Err(bad());
            }
        }
        Ok(())
    }
}
