//! One same-root Shared parent: five provider roles and an optional actual guard owner.
mod exits;
mod guards;
use super::*;
use crate::ssa::model::{Edge, Function};
pub(super) use exits::NestedState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ParentRoles {
    pub(super) block: BlockId,
    pub(super) caller: EntityId,
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) guard: Option<ValueId>,
}

#[derive(Clone, Copy)]
pub(super) struct ParentCheckpoint {
    statement: UnitStatementId,
    roles: ParentRoles,
    retained_start: usize,
    phase: MapPhase,
}

#[derive(Clone, Copy)]
pub(super) struct ParentTransport {
    checkpoint: ParentCheckpoint,
    header: ParentRoles,
    body: ParentRoles,
    exhaustion: ParentRoles,
}

pub(super) struct Edges<'a> {
    pub(super) entry: &'a Edge,
    pub(super) body: &'a Edge,
    pub(super) exhaustion: &'a Edge,
}

impl ParentRoles {
    fn read(
        lowerer: &UnitExpressionLowerer<'_>,
        state: MapState,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let roles = Self {
            block: lowerer.block,
            caller: state.carrier.identity.current(lowerer, span)?,
            source: lowerer.map_pending_source(state.source_slot, span)?,
            cursor: lowerer.map_pending_cursor(state.cursor_slot, state.cursor_type, span)?,
            key: lowerer.map_pending_source(state.key_slot, span)?,
            value: lowerer.map_pending_source(state.value_slot, span)?,
            guard: state
                .carrier
                .nested
                .and_then(|pair| pair.tail)
                .map(|tail| lowerer.nested_guard_current(tail.guards[0], span))
                .transpose()?,
        };
        if !matches!(roles.caller, EntityId::Loan(_))
            || lowerer.borrow_bindings.get(&state.key_binding) != Some(&roles.key)
            || lowerer.borrow_bindings.get(&state.value_binding) != Some(&roles.value)
            || roles.entities().into_iter().collect::<BTreeSet<_>>().len()
                != 5 + usize::from(roles.guard.is_some())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(roles)
    }

    fn entities(self) -> Vec<EntityId> {
        let mut entities = vec![
            self.caller,
            EntityId::Loan(self.source),
            EntityId::Value(self.cursor),
            EntityId::Loan(self.key),
            EntityId::Loan(self.value),
        ];
        entities.extend(self.guard.map(EntityId::Value));
        entities
    }

    fn transported(
        self,
        function: &Function,
        edge: &Edge,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let [caller, source, cursor, key, value] = [
            self.caller,
            EntityId::Loan(self.source),
            EntityId::Value(self.cursor),
            EntityId::Loan(self.key),
            EntityId::Loan(self.value),
        ]
        .map(|entity| transport::along(function, edge, entity, span));
        let guard = self
            .guard
            .map(
                |guard| match transport::along(function, edge, EntityId::Value(guard), span)? {
                    EntityId::Value(value) => Ok(value),
                    _ => Err(bad()),
                },
            )
            .transpose()?;
        let (
            EntityId::Loan(source),
            EntityId::Value(cursor),
            EntityId::Loan(key),
            EntityId::Loan(value),
        ) = (source?, cursor?, key?, value?)
        else {
            return Err(bad());
        };
        Ok(Self {
            block: edge.target,
            caller: caller?,
            source,
            cursor,
            key,
            value,
            guard,
        })
    }
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn checkpoint_nested_map(
        &mut self,
        qualified: map_iteration::QualifiedMap,
        span: Span,
    ) -> Result<Option<ParentCheckpoint>, LoweringError> {
        let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if self.loops.is_empty() && self.pending_operands.is_empty() {
            if qualified
                .nested
                .is_some_and(|pair| pair.inner == qualified.statement)
            {
                return Err(bad());
            }
            return Ok(None);
        }
        let Some(pair) = qualified.nested else {
            return Err(unsupported());
        };
        let [context] = self.loops.as_slice() else {
            return Err(unsupported());
        };
        let Some(iteration) = context.iteration.as_ref() else {
            return Err(unsupported());
        };
        let IterationState::Map(state) = &iteration.state else {
            return Err(unsupported());
        };
        if qualified.statement != pair.inner || iteration.statement != pair.outer {
            return Err(unsupported());
        }
        if state.carrier.nested != Some(pair)
            || state.carrier.identity != qualified.source
            || state.carrier.resumed.is_some()
            || state.conditional.is_some()
            || iteration.retained_start != 0
            || [
                state.source_slot,
                state.cursor_slot,
                state.key_slot,
                state.value_slot,
            ] != [0, 1, 2, 3]
            || self.pending_operands.len() != 4
            || !context.continues.is_empty()
            || !context.breaks.is_empty()
            || !matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            )
        {
            return Err(bad());
        }
        self.validate_map_state_current(**state, state.carrier.body, span)?;
        let roles = ParentRoles::read(self, **state, span)?;
        for (loan, operation) in [
            (
                roles.key,
                Operation::MapIterationKeyLoan {
                    source: roles.source,
                    cursor: roles.cursor,
                },
            ),
            (
                roles.value,
                Operation::MapIterationValueLoan {
                    source: roles.source,
                    cursor: roles.cursor,
                },
            ),
        ] {
            let instruction = carrier::instruction(self.function, EntityId::Loan(loan), span)?;
            if instruction.block != state.carrier.body || instruction.operation != operation {
                return Err(bad());
            }
        }
        let phase = state.phase;
        if let Some(tail) = pair.tail {
            let owner = self.nested_guard_birth(tail.guards[0], roles.block, span)?;
            if roles.guard != Some(owner) {
                return Err(bad());
            }
            self.nested_state_mut(pair.outer, span)?.guard_birth = Some(owner);
        }
        Ok(Some(ParentCheckpoint {
            statement: pair.outer,
            roles,
            retained_start: self.pending_operands.len(),
            phase,
        }))
    }

    fn restore_nested_parent(
        &mut self,
        checkpoint: ParentCheckpoint,
        roles: ParentRoles,
        restore_false: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let state = self.map_state(checkpoint.statement, span)?;
        if ParentRoles::read(self, state, span)? != roles
            || !(matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            ) || (restore_false && self.nested_parent_return_completed(state, span)?))
            || !matches!(
                checkpoint.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            )
            || self.pending_operands.len() < checkpoint.retained_start
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let iteration = self
            .loops
            .iter_mut()
            .filter_map(|context| context.iteration.as_mut())
            .find(|iteration| iteration.statement == checkpoint.statement)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let IterationState::Map(state) = &mut iteration.state else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        // Restore path-local roles only. The parent's exits/evidence are never rolled back.
        state.phase = checkpoint.phase;
        state.carrier.resumed = Some(roles);
        let state = **state;
        self.validate_map_state_current(state, roles.block, span)
    }

    pub(super) fn validate_nested_component(
        &self,
        state: MapState,
        symbol: UnitSymbolId,
        loan: LoanId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if state.carrier.nested.is_none() {
            return Ok(());
        }
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        self.validate_map_state_current(state, state.body_block(), span)?;
        if let Some(roles) = state.carrier.resumed {
            let expected = if symbol == state.key_binding {
                roles.key
            } else if symbol == state.value_binding {
                roles.value
            } else {
                return Err(bad());
            };
            if roles.block != self.block || loan != expected {
                return Err(bad());
            }
        } else {
            let source = self.map_pending_source(state.source_slot, span)?;
            let cursor = self.map_pending_cursor(state.cursor_slot, state.cursor_type, span)?;
            let operation = if symbol == state.key_binding {
                Operation::MapIterationKeyLoan { source, cursor }
            } else if symbol == state.value_binding {
                Operation::MapIterationValueLoan { source, cursor }
            } else {
                return Err(bad());
            };
            let projection = carrier::instruction(self.function, EntityId::Loan(loan), span)?;
            if projection.block != state.carrier.body || projection.operation != operation {
                return Err(bad());
            }
        }
        Ok(())
    }
}

impl ParentTransport {
    pub(super) fn new(
        lowerer: &UnitExpressionLowerer<'_>,
        checkpoint: ParentCheckpoint,
        edges: Edges<'_>,
        source: LoanId,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if checkpoint
            .roles
            .entities()
            .contains(&EntityId::Loan(source))
        {
            return Err(bad());
        }
        let birth = carrier::instruction(lowerer.function, EntityId::Loan(source), span)?;
        let EntityId::Loan(caller) = checkpoint.roles.caller else {
            return Err(bad());
        };
        if birth.block != checkpoint.roles.block
            || birth.operation != (Operation::SharedReborrow { source: caller })
        {
            return Err(bad());
        }
        let header = checkpoint
            .roles
            .transported(lowerer.function, edges.entry, span)?;
        let body = header.transported(lowerer.function, edges.body, span)?;
        let exhaustion = header.transported(lowerer.function, edges.exhaustion, span)?;
        if header.block != lowerer.block {
            return Err(bad());
        }
        for (before, after) in [
            (checkpoint.roles, header),
            (header, body),
            (header, exhaustion),
        ] {
            let incoming = transport::incoming(lowerer.function, after.block);
            let [(from, actual)] = incoming.as_slice() else {
                return Err(bad());
            };
            if *from != before.block || before.transported(lowerer.function, actual, span)? != after
            {
                return Err(bad());
            }
        }
        Ok(Self {
            checkpoint,
            header,
            body,
            exhaustion,
        })
    }

    pub(super) fn enter_header(
        self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        lowerer.restore_nested_parent(self.checkpoint, self.header, false, span)
    }
    pub(super) fn enter_body(
        self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        lowerer.restore_nested_parent(self.checkpoint, self.body, false, span)
    }
    pub(super) fn enter_exhaustion(
        self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        lowerer.restore_nested_parent(self.checkpoint, self.exhaustion, true, span)
    }

    pub(super) fn validate_body_exit(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        kind: UnitIterationExitKind,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let state = lowerer.map_state(self.checkpoint.statement, span)?;
        let returning = matches!(kind, UnitIterationExitKind::Return(_));
        let backedge = matches!(
            kind,
            UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Continue(_)
        );
        if returning {
            if !lowerer.nested_parent_return_completed(state, span)? {
                return Err(bad());
            }
        } else if ParentRoles::read(lowerer, state, span)? != self.body {
            return Err(bad());
        }
        if !returning {
            let block = lowerer.function.block(self.body.block).ok_or_else(bad)?;
            for id in &block.instructions {
                let operation = &lowerer.function.instruction(*id).ok_or_else(bad)?.operation;
                if matches!(operation, Operation::BorrowEnd { loan }
                    if [self.body.source, self.body.key, self.body.value].contains(loan)
                        || EntityId::Loan(*loan) == self.body.caller)
                    || matches!(operation, Operation::Drop { owner } if Some(*owner) == self.body.guard)
                    || matches!(operation, Operation::MapIterationNext { source, .. } if *source == self.body.source)
                {
                    return Err(bad());
                }
            }
        }
        let incoming = transport::incoming(lowerer.function, self.header.block);
        if incoming.len() != 1 + usize::from(backedge) {
            return Err(bad());
        }
        let mut seen_entry = false;
        let mut seen_backedge = false;
        for (from, edge) in incoming {
            let roles = if from == self.checkpoint.roles.block && !seen_entry {
                seen_entry = true;
                self.checkpoint.roles
            } else if backedge && from == self.body.block && !seen_backedge {
                seen_backedge = true;
                self.body
            } else {
                return Err(bad());
            };
            if roles.transported(lowerer.function, edge, span)? != self.header {
                return Err(bad());
            }
        }
        if !seen_entry || seen_backedge != backedge {
            return Err(bad());
        }
        Ok(())
    }

    pub(super) fn enter_continuation(
        self,
        lowerer: &mut UnitExpressionLowerer<'_>,
        breaking: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !breaking {
            return self.validate_retained(lowerer, span);
        }
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let state = lowerer.map_state(self.checkpoint.statement, span)?;
        let roles = ParentRoles::read(lowerer, state, span)?;
        let incoming = transport::incoming(lowerer.function, roles.block);
        if incoming.len() != 2
            || roles.block == self.body.block
            || roles.block == self.exhaustion.block
        {
            return Err(bad());
        }
        for expected in [self.body, self.exhaustion] {
            let edges = incoming
                .iter()
                .filter(|(from, _)| *from == expected.block)
                .collect::<Vec<_>>();
            let [(_, edge)] = edges.as_slice() else {
                return Err(bad());
            };
            if expected.transported(lowerer.function, edge, span)? != roles {
                return Err(bad());
            }
        }
        lowerer.restore_nested_parent(self.checkpoint, roles, false, span)?;
        if lowerer.pending_operands.len() != self.checkpoint.retained_start {
            return Err(bad());
        }
        if let (Some(current), Some(anchor)) = (roles.guard, self.checkpoint.roles.guard) {
            transport::from_anchor(
                lowerer.function,
                EntityId::Value(current),
                EntityId::Value(anchor),
                span,
            )?;
        }
        Ok(())
    }

    pub(super) fn validate_retained(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let state = lowerer.map_state(self.checkpoint.statement, span)?;
        if lowerer.pending_operands.len() != self.checkpoint.retained_start
            || ParentRoles::read(lowerer, state, span)? != self.exhaustion
            || state.carrier.resumed != Some(self.exhaustion)
            || !matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            )
        {
            return Err(bad());
        }
        lowerer.validate_map_state_current(state, self.exhaustion.block, span)
    }
}
