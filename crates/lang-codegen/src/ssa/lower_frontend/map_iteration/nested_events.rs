//! One real cleanup point owns its complete sequence, including both Return frames.
use super::*;
use lang_frontend::ownership_checking::DropFact;

#[derive(Clone, Copy)]
pub(super) struct Participant {
    pub(super) statement: StatementId,
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) guard: ValueId,
}

/// Append-only evidence. Restoring the sibling false path never removes a record.
#[derive(Clone, Copy)]
pub(super) struct Event {
    pub(super) kind: IterationExitKind,
    pub(super) statement: StatementId,
    pub(super) block: BlockId,
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) guard: ValueId,
    pub(super) peer: Option<Participant>,
    pub(super) cleanup_start: usize,
    pub(super) cleanup_end: usize,
    pub(super) returned: Option<ValueId>,
    pub(super) operand: Option<(Span, crate::ssa::model::FunctionId)>,
}

impl Event {
    fn participant(self, statement: StatementId, span: Span) -> Result<Participant, LoweringError> {
        if statement == self.statement {
            Ok(Participant {
                statement,
                source: self.source,
                cursor: self.cursor,
                key: self.key,
                value: self.value,
                guard: self.guard,
            })
        } else {
            self.peer
                .filter(|peer| peer.statement == statement)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }
    }
}

impl ExpressionLowerer<'_> {
    /// Return true only for the fully qualified new pair. Ordinary paths are unchanged.
    pub(super) fn clean_nested_map_point(
        &mut self,
        kind: IterationExitKind,
        returned: Option<ValueId>,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let Some(pair) = self
            .loops
            .last()
            .and_then(|context| context.map_loop.as_ref())
            .and_then(|state| state.nested.as_ref())
            .map(|nested| nested.pair)
        else {
            return Ok(false);
        };
        let Some(shape) = pair.tail else {
            return Ok(false);
        };
        let current = self
            .loops
            .last()
            .and_then(|context| context.map_loop.as_ref())
            .ok_or_else(missing)?;
        let statement = current.frame.statement;
        if (statement == pair.inner && kind != shape.kind)
            || (statement == pair.outer && kind != IterationExitKind::Fallthrough)
            || (matches!(kind, IterationExitKind::Return(_)) != returned.is_some())
            || current.kind != kind
            || current.cleaned
            || current.body != self.block
        {
            return Err(missing());
        }
        let exit = unique(
            current
                .plan
                .exits()
                .iter()
                .filter(|exit| exit.kind() == kind),
        )
        .ok_or_else(missing)?
        .clone();
        let point = exit.point();
        let actions = self
            .owned
            .cleanup_steps()
            .iter()
            .filter_map(|(actual, action)| (*actual == point).then_some(*action))
            .collect::<Vec<_>>();
        if actions != exit.actions() {
            return Err(missing());
        }
        let operand = if returned.is_some() {
            let expression = shape.operand.ok_or_else(missing)?;
            let origin = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| missing())?
                .span();
            let call = self.typed.call(expression).ok_or_else(missing)?;
            let lang_frontend::type_checking::CallableTarget::Source(symbol) = call.target() else {
                return Err(missing());
            };
            let callee = *self
                .function_ids
                .get(&FunctionInstanceKey::new(symbol, Vec::new()))
                .ok_or_else(missing)?;
            Some((origin, callee))
        } else {
            None
        };
        let statements = if returned.is_some() {
            vec![pair.inner, pair.outer]
        } else {
            vec![statement]
        };
        let start = self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .instructions
            .len();
        let mut participants = Vec::new();
        for &statement in &statements {
            let state = self
                .loops
                .iter()
                .filter_map(|context| context.map_loop.as_ref())
                .find(|state| state.frame.statement == statement)
                .ok_or_else(missing)?;
            if state.nested.as_ref().map(|nested| nested.pair) != Some(pair)
                || state.cleaned
                || state.frame.finished
                || state.body != self.block
                || state.nested_events.iter().any(|event| event.kind == kind)
            {
                return Err(missing());
            }
            let [(key_symbol, Some(key)), (value_symbol, Some(value))] = state.frame.components
            else {
                return Err(missing());
            };
            if self.borrow_bindings.get(&key_symbol) != Some(&key)
                || self.borrow_bindings.get(&value_symbol) != Some(&value)
            {
                return Err(missing());
            }
            let local = if statement == pair.outer {
                shape.outer_guard
            } else {
                shape.inner_guard
            };
            let Some(LoweredValue::Value(guard)) = self.bindings.get(&local.symbol).copied() else {
                return Err(missing());
            };
            if statement == pair.outer {
                if nested::current_roles(self, state, span)?.guard != Some(guard) {
                    return Err(missing());
                }
            } else {
                let ty = self.expression_ssa_type(local.initializer, span)?;
                let resolved = self.resolve_type(local.ty, span)?;
                let birth = carrier::definition(self.function, EntityId::Value(guard))
                    .ok_or_else(missing)?;
                let Operation::HeapAllocate { owner, payload } = birth.operation else {
                    return Err(missing());
                };
                let payload = carrier::definition(self.function, EntityId::Value(payload))
                    .ok_or_else(missing)?;
                let expected_payload = self.heap_payloads.get(&ty).ok_or_else(missing)?;
                if birth.block != self.block
                    || birth.origin != Origin::Source(local.initializer_span)
                    || owner != ty
                    || self.type_ids.get(&resolved) != Some(&ty)
                    || self
                        .function
                        .entity(EntityId::Value(guard))
                        .map(|entity| entity.ty)
                        != Some(EntityType::Value(ty))
                    || payload.block != birth.block
                    || payload.origin != birth.origin
                    || !matches!(&payload.operation, Operation::AggregateConstruct { aggregate, fields }
                        if aggregate == expected_payload && fields.is_empty())
                    || self
                        .function
                        .instructions
                        .iter()
                        .filter(|instruction| {
                            instruction.origin == birth.origin
                                && matches!(instruction.operation, Operation::HeapAllocate { .. })
                        })
                        .count()
                        != 1
                {
                    return Err(missing());
                }
            }
            participants.push(Participant {
                statement,
                source: state.frame.source.ok_or_else(missing)?,
                cursor: state.cursor,
                key,
                value,
                guard,
            });
        }
        // The parent return is a reference to this same complete point, never a second sequence.
        if returned.is_some() {
            let parent = self
                .loops
                .first()
                .and_then(|context| context.map_loop.as_ref())
                .ok_or_else(missing)?;
            let parent_exit = unique(
                parent
                    .plan
                    .exits()
                    .iter()
                    .filter(|exit| exit.kind() == kind),
            )
            .ok_or_else(missing)?;
            if parent_exit.point() != point || parent_exit.actions() != actions {
                return Err(missing());
            }
        }
        for action in &actions {
            match *action {
                IterationCleanupAction::Drop(fact) => {
                    let (participant, local) = if fact == shape.inner_drop {
                        (
                            participants.iter().find(|p| p.statement == pair.inner),
                            shape.inner_guard,
                        )
                    } else if fact == shape.outer_fallthrough_drop
                        || Some(fact) == shape.outer_return_drop
                    {
                        (
                            participants.iter().find(|p| p.statement == pair.outer),
                            shape.outer_guard,
                        )
                    } else {
                        return Err(missing());
                    };
                    let participant = participant.ok_or_else(missing)?;
                    if fact.point() != point
                        || fact.value_origin() != local.origin
                        || self.bindings.remove(&local.symbol)
                            != Some(LoweredValue::Value(participant.guard))
                    {
                        return Err(missing());
                    }
                    self.append(
                        Operation::Drop {
                            owner: participant.guard,
                        },
                        Vec::new(),
                        fact.value_origin(),
                    )?;
                }
                IterationCleanupAction::EndBinding { statement, symbol } => {
                    let state = self
                        .loops
                        .iter_mut()
                        .filter_map(|context| context.map_loop.as_mut())
                        .find(|state| state.frame.statement == statement)
                        .ok_or_else(missing)?;
                    if !statements.contains(&statement) || state.frame.finished {
                        return Err(missing());
                    }
                    let index = state
                        .frame
                        .components
                        .iter()
                        .position(|(actual, _)| *actual == symbol)
                        .ok_or_else(missing)?;
                    if index == 0 && state.frame.components[1].1.is_some() {
                        return Err(missing());
                    }
                    let loan = state.frame.components[index].1.take().ok_or_else(missing)?;
                    if self.borrow_bindings.remove(&symbol) != Some(loan) {
                        return Err(missing());
                    }
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
                IterationCleanupAction::FinishProvider(statement) => {
                    let state = self
                        .loops
                        .iter_mut()
                        .filter_map(|context| context.map_loop.as_mut())
                        .find(|state| state.frame.statement == statement)
                        .ok_or_else(missing)?;
                    if !statements.contains(&statement)
                        || state.frame.finished
                        || state.frame.source.is_none()
                        || state
                            .frame
                            .components
                            .iter()
                            .any(|(_, loan)| loan.is_some())
                    {
                        return Err(missing());
                    }
                    state.frame.finished = true;
                }
                IterationCleanupAction::EndSource(statement) => {
                    let state = self
                        .loops
                        .iter_mut()
                        .filter_map(|context| context.map_loop.as_mut())
                        .find(|state| state.frame.statement == statement)
                        .ok_or_else(missing)?;
                    if !statements.contains(&statement) || !state.frame.finished {
                        return Err(missing());
                    }
                    let loan = state.frame.source.take().ok_or_else(missing)?;
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
                _ => return Err(missing()),
            }
        }
        let end = self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .instructions
            .len();
        for participant in &participants {
            let state = self
                .loops
                .iter_mut()
                .filter_map(|context| context.map_loop.as_mut())
                .find(|state| state.frame.statement == participant.statement)
                .ok_or_else(missing)?;
            let terminal = exit_shape::terminal(kind);
            if state
                .frame
                .components
                .iter()
                .any(|(_, loan)| loan.is_some())
                || state.frame.finished != terminal
                || state.frame.source.is_none() != terminal
            {
                return Err(missing());
            }
            state.nested_events.push(Event {
                kind,
                statement: participant.statement,
                block: self.block,
                source: participant.source,
                cursor: participant.cursor,
                key: participant.key,
                value: participant.value,
                guard: participant.guard,
                peer: participants
                    .iter()
                    .find(|other| other.statement != participant.statement)
                    .copied(),
                cleanup_start: start,
                cleanup_end: end,
                returned,
                operand,
            });
            state.cleaned = true;
            state.cleanup_start = Some(start);
            if participant.statement == statement {
                state.returned = returned;
            }
        }
        Ok(true)
    }
}

/// Validate every recorded sequence against concrete SSA instructions, not a success flag.
pub(super) fn validate(
    e: carrier::Evidence<'_>,
    state: &nested::State,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, e.span);
    let pair = state.pair;
    let shape = pair.tail.ok_or_else(missing)?;
    let inner = state.parent.is_some();
    let statement = if inner { pair.inner } else { pair.outer };
    let expected = if inner {
        vec![shape.kind]
    } else if matches!(shape.kind, IterationExitKind::Return(_)) {
        vec![shape.kind, IterationExitKind::Fallthrough]
    } else {
        vec![IterationExitKind::Fallthrough]
    };
    if e.events.iter().map(|event| event.kind).collect::<Vec<_>>() != expected
        || e.events.iter().any(|event| event.statement != statement)
    {
        return Err(missing());
    }
    for event in e.events {
        validate_event(e, pair, *event)?;
    }
    let primary = e.events.last().ok_or_else(missing)?;
    if primary.block != e.cleanup_body
        || primary.returned != e.returned
        || Some(primary.cleanup_start) != e.cleanup_start
    {
        return Err(missing());
    }
    let operations = carrier::instructions(e.function, primary.block).ok_or_else(missing)?;
    match primary.kind {
        IterationExitKind::Fallthrough | IterationExitKind::Continue(_) => {
            let next = e.next.ok_or_else(missing)?;
            let instruction =
                carrier::definition(e.function, EntityId::Value(next)).ok_or_else(missing)?;
            let position = operations
                .iter()
                .position(|i| i.id == instruction.id)
                .ok_or_else(missing)?;
            let Some(TerminatorKind::Branch(edge)) = carrier::terminator(e.function, primary.block)
            else {
                return Err(missing());
            };
            if instruction.block != primary.block
                || position < primary.cleanup_end
                || instruction.operation
                    != (Operation::MapIterationNext {
                        source: primary.source,
                        cursor: primary.cursor,
                    })
                || operations
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
                    .count()
                    != 1
                || edge.target != e.blocks[1]
                || edge.arguments.get(e.source_slot) != Some(&EntityId::Loan(primary.source))
                || edge.arguments.get(e.cursor_slot) != Some(&EntityId::Value(next))
            {
                return Err(missing());
            }
        }
        IterationExitKind::Break(_) | IterationExitKind::Return(_) => {
            if e.next.is_some()
                || operations
                    .iter()
                    .any(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
            {
                return Err(missing());
            }
        }
        _ => return Err(missing()),
    }
    let exhausted = e.blocks[3];
    let parameters = &e.function.block(exhausted).ok_or_else(missing)?.parameters;
    let (source, cursor) = roles(parameters, e.source_slot, e.cursor_slot, e.span)?;
    let ops = carrier::instructions(e.function, exhausted).ok_or_else(missing)?;
    if ops
        .iter()
        .filter(|i| i.operation == Operation::BorrowEnd { loan: source })
        .count()
        != 1
        || ops.iter().any(|i| {
            matches!(
                i.operation,
                Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. }
            ) || matches!(i.operation, Operation::MapIterationNext { source: actual_source, cursor: actual_cursor }
                if !inner || actual_source == source || actual_cursor == cursor)
        })
    {
        return Err(missing());
    }
    Ok(())
}

fn validate_event(
    e: carrier::Evidence<'_>,
    pair: nested::Pair,
    event: Event,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, e.span);
    let shape = pair.tail.ok_or_else(missing)?;
    let operations = carrier::instructions(e.function, event.block).ok_or_else(missing)?;
    if exit_shape::terminal(event.kind)
        && operations
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::MapIterationNext { .. }))
    {
        return Err(missing());
    }
    let slice = operations
        .get(event.cleanup_start..event.cleanup_end)
        .ok_or_else(missing)?;
    let current = event.participant(event.statement, e.span)?;
    let mut expected = Vec::<(Operation, Option<Span>)>::new();
    let mut add = |participant: Participant, drop: DropFact, terminal: bool| {
        expected.push((
            Operation::Drop {
                owner: participant.guard,
            },
            Some(drop.value_origin()),
        ));
        expected.push((
            Operation::BorrowEnd {
                loan: participant.value,
            },
            None,
        ));
        expected.push((
            Operation::BorrowEnd {
                loan: participant.key,
            },
            None,
        ));
        if terminal {
            expected.push((
                Operation::BorrowEnd {
                    loan: participant.source,
                },
                None,
            ));
        }
    };
    match event.kind {
        IterationExitKind::Fallthrough => {
            if event.statement != pair.outer
                || event.peer.is_some()
                || event.returned.is_some()
                || event.operand.is_some()
            {
                return Err(missing());
            }
            add(current, shape.outer_fallthrough_drop, false);
        }
        IterationExitKind::Continue(_) | IterationExitKind::Break(_) => {
            if event.statement != pair.inner
                || event.kind != shape.kind
                || event.peer.is_some()
                || event.returned.is_some()
                || event.operand.is_some()
            {
                return Err(missing());
            }
            add(current, shape.inner_drop, exit_shape::terminal(event.kind));
        }
        IterationExitKind::Return(_) => {
            if event.kind != shape.kind {
                return Err(missing());
            }
            add(
                event.participant(pair.inner, e.span)?,
                shape.inner_drop,
                true,
            );
            add(
                event.participant(pair.outer, e.span)?,
                shape.outer_return_drop.ok_or_else(missing)?,
                true,
            );
            let returned = event.returned.ok_or_else(missing)?;
            let (operand_span, callee) = event.operand.ok_or_else(missing)?;
            let call =
                carrier::definition(e.function, EntityId::Value(returned)).ok_or_else(missing)?;
            let position = operations
                .iter()
                .position(|i| i.id == call.id)
                .ok_or_else(missing)?;
            let outer = event.participant(pair.outer, e.span)?;
            let inner = event.participant(pair.inner, e.span)?;
            let Some(TerminatorKind::Return { values }) =
                carrier::terminator(e.function, event.block)
            else {
                return Err(missing());
            };
            if position >= event.cleanup_start
                || call.block != event.block
                || call.origin != Origin::Source(operand_span)
                || e.function
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        instruction.origin == call.origin
                            && matches!(instruction.operation, Operation::DirectCall { .. })
                    })
                    .count()
                    != 1
                || values.as_slice() != [returned]
                || !matches!(&call.operation, Operation::DirectCall { callee: actual, receiver: None, arguments }
                    if *actual == callee && arguments.as_slice() == [EntityId::Loan(outer.key), EntityId::Loan(outer.value),
                        EntityId::Loan(inner.key), EntityId::Loan(inner.value)])
            {
                return Err(missing());
            }
        }
        _ => return Err(missing()),
    }
    if slice.len() != expected.len()
        || slice
            .iter()
            .zip(expected)
            .any(|(actual, (operation, origin))| {
                actual.operation != operation
                    || origin.is_some_and(|origin| actual.origin != Origin::Source(origin))
            })
    {
        return Err(missing());
    }
    for participant in [Some(current), event.peer].into_iter().flatten() {
        for (operation, count) in [
            (
                Operation::Drop {
                    owner: participant.guard,
                },
                1,
            ),
            (
                Operation::BorrowEnd {
                    loan: participant.key,
                },
                1,
            ),
            (
                Operation::BorrowEnd {
                    loan: participant.value,
                },
                1,
            ),
            (
                Operation::BorrowEnd {
                    loan: participant.source,
                },
                usize::from(exit_shape::terminal(event.kind)),
            ),
        ] {
            if operations
                .iter()
                .filter(|instruction| instruction.operation == operation)
                .count()
                != count
            {
                return Err(missing());
            }
        }
        if operations[..event.cleanup_start].iter().any(|instruction| {
            instruction.operation
                == Operation::Drop {
                    owner: participant.guard,
                }
                || matches!(instruction.operation, Operation::BorrowEnd { loan }
                    if [participant.source, participant.key, participant.value].contains(&loan))
        }) {
            return Err(missing());
        }
    }
    Ok(())
}
