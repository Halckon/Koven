//! Verify concrete CFG operands, not fixed birth-column numbers after an If.
use super::super::carrier::{Evidence, definition, instructions, terminator};
use super::transport::forwarded;
use super::*;

pub(in super::super) fn validate(e: Evidence<'_>, state: &State) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, e.span);
    let [preheader, header, body, exhausted] = e.blocks;
    if state.transfers.len() != 6 || state.exits.len() != 4 {
        return Err(missing());
    }
    let body_parameters = &e.function.block(body).ok_or_else(missing)?.parameters;
    let (body_source, body_cursor) =
        super::super::roles(body_parameters, e.source_slot, e.cursor_slot, e.span)?;
    let first = state.transfers.first().ok_or_else(missing)?;
    if first.from != body
        || first.before.source != body_source
        || first.before.cursor != body_cursor
        || first.before.key != e.key
        || first.before.value != e.value
        || first.before.owner != e.body_owner.map(|(_, _, owner)| owner)
        || definition(e.function, EntityId::Value(first.before.local)).is_none_or(|instruction| {
            instruction.block != body
                || instruction.origin != Origin::Source(state.shape.local.initializer_span)
        })
    {
        return Err(missing());
    }
    let mut from = body;
    let mut before = first.before;
    for (index, arm) in state.shape.arms.iter().enumerate() {
        let pair = &state.transfers[index * 2..index * 2 + 2];
        let Some(TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        }) = terminator(e.function, from)
        else {
            return Err(missing());
        };
        let comparison = definition(e.function, EntityId::Value(*condition)).ok_or_else(missing)?;
        let Operation::StringEqual {
            left,
            right: EntityId::Value(literal),
        } = comparison.operation
        else {
            return Err(missing());
        };
        let literal_instruction =
            definition(e.function, EntityId::Value(literal)).ok_or_else(missing)?;
        let from_instructions = instructions(e.function, from).ok_or_else(missing)?;
        let compare_at = from_instructions
            .iter()
            .position(|instruction| instruction.id == comparison.id)
            .ok_or_else(missing)?;
        let literal_drop = unique(from_instructions.iter().enumerate().filter_map(
            |(position, instruction)| {
                (instruction.operation == Operation::Drop { owner: literal }).then_some(position)
            },
        ))
        .ok_or_else(missing)?;
        if comparison.block != from || left != EntityId::Loan(before.key) || literal_instruction.block != from
            || !matches!(literal_instruction.operation, Operation::StringLiteral { .. }) || literal_drop <= compare_at
            || from_instructions.iter().any(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan } if [before.source,before.key,before.value].contains(&loan))
                    || instruction.operation == Operation::Drop { owner: before.local }
                    || before.owner.is_some_and(|owner| instruction.operation == Operation::Drop { owner })
                    || matches!(instruction.operation, Operation::MapIterationNext { .. })
            }) { return Err(missing()); }
        for (branch, (transfer, edge)) in pair.iter().zip([when_true, when_false]).enumerate() {
            if transfer.control != arm.control
                || transfer.branch != branch
                || transfer.from != from
                || transfer.before != before
                || edge.target != transfer.to
            {
                return Err(missing());
            }
            let parameters = &e
                .function
                .block(transfer.to)
                .ok_or_else(missing)?
                .parameters;
            if transfer.after != forwarded(before, &edge.arguments, parameters, e.span)? {
                return Err(missing());
            }
        }
        let exit = &state.exits[index];
        if exit.kind != arm.kind || exit.block != pair[0].to || exit.roles != pair[0].after {
            return Err(missing());
        }
        from = pair[1].to;
        before = pair[1].after;
    }
    let tail = &state.exits[3];
    if tail.kind != IterationExitKind::Fallthrough || tail.block != from || tail.roles != before {
        return Err(missing());
    }
    for exit in &state.exits {
        check_exit(e, state.shape, exit)?;
    }
    let exhausted_parameters = &e.function.block(exhausted).ok_or_else(missing)?.parameters;
    let (exhausted_source, _) =
        super::super::roles(exhausted_parameters, e.source_slot, e.cursor_slot, e.span)?;
    let exhausted_instructions = instructions(e.function, exhausted).ok_or_else(missing)?;
    let source_end = unique(exhausted_instructions.iter().enumerate().filter_map(
        |(position, instruction)| {
            (instruction.operation
                == Operation::BorrowEnd {
                    loan: exhausted_source,
                })
            .then_some(position)
        },
    ))
    .ok_or_else(missing)?;
    if exhausted_instructions.iter().any(|instruction| {
        matches!(
            instruction.operation,
            Operation::MapIterationKeyLoan { .. }
                | Operation::MapIterationValueLoan { .. }
                | Operation::MapIterationNext { .. }
        )
    }) {
        return Err(missing());
    }
    let Some(TerminatorKind::Branch(edge)) = terminator(e.function, exhausted) else {
        return Err(missing());
    };
    if edge.target != e.continuation || e.continuation == exhausted || e.continuation == header {
        return Err(missing());
    }
    if let Some((proof, symbol, dropped)) = e.owner {
        let (slot, binding) = unique(
            e.carried
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.symbol == Some(symbol)),
        )
        .ok_or_else(missing)?;
        let Some(TerminatorKind::Branch(entry)) = terminator(e.function, preheader) else {
            return Err(missing());
        };
        if binding.source != e.seed
            || entry.arguments.get(slot) != Some(&e.seed)
            || exhausted_parameters.get(slot) != Some(&EntityId::Value(dropped))
            || body_parameters.get(slot)
                != e.body_owner
                    .map(|(_, _, owner)| EntityId::Value(owner))
                    .as_ref()
            || proof.drop.value_origin() != e.parameter_origin
        {
            return Err(missing());
        }
        let (position, instruction) = unique(exhausted_instructions.iter().enumerate().filter(
            |(_, instruction)| instruction.operation == Operation::Drop { owner: dropped },
        ))
        .ok_or_else(missing)?;
        if position <= source_end || instruction.origin != Origin::Source(e.parameter_origin) {
            return Err(missing());
        }
    } else if !matches!(e.seed, EntityId::Loan(_)) {
        return Err(missing());
    }
    if let EntityId::Loan(entry) = e.seed {
        if e.body_owner.is_some()
            || state
                .transfers
                .iter()
                .any(|transfer| transfer.before.owner.is_some() || transfer.after.owner.is_some())
        {
            return Err(missing());
        }
        for block in state
            .transfers
            .iter()
            .flat_map(|transfer| [transfer.from, transfer.to])
            .chain([exhausted])
        {
            if instructions(e.function, block)
                .ok_or_else(missing)?
                .iter()
                .any(|instruction| instruction.operation == Operation::BorrowEnd { loan: entry })
            {
                return Err(missing());
            }
        }
    }
    Ok(())
}

fn check_exit(e: Evidence<'_>, shape: Shape, exit: &Exit) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, e.span);
    let body = instructions(e.function, exit.block).ok_or_else(missing)?;
    let position = |operation| {
        unique(
            body.iter()
                .enumerate()
                .filter_map(|(position, instruction)| {
                    (instruction.operation == operation).then_some(position)
                }),
        )
        .ok_or_else(missing)
    };
    let local_drop = position(Operation::Drop {
        owner: exit.roles.local,
    })?;
    let value_end = position(Operation::BorrowEnd {
        loan: exit.roles.value,
    })?;
    let key_end = position(Operation::BorrowEnd {
        loan: exit.roles.key,
    })?;
    if local_drop != exit.cleanup_start
        || local_drop >= value_end
        || value_end >= key_end
        || body[local_drop].origin != Origin::Source(shape.local.origin)
    {
        return Err(missing());
    }
    let source_ends: Vec<_> = body
        .iter()
        .enumerate()
        .filter_map(|(position, instruction)| {
            (instruction.operation
                == Operation::BorrowEnd {
                    loan: exit.roles.source,
                })
            .then_some(position)
        })
        .collect();
    let owner_drops: Vec<_> = body
        .iter()
        .enumerate()
        .filter_map(|(position, instruction)| {
            exit.roles
                .owner
                .filter(|owner| instruction.operation == Operation::Drop { owner: *owner })
                .map(|_| position)
        })
        .collect();
    if exit_shape::terminal(exit.kind) {
        let [source_end] = source_ends.as_slice() else {
            return Err(missing());
        };
        if *source_end <= key_end
            || exit.next.is_some()
            || body.iter().any(|instruction| {
                matches!(instruction.operation, Operation::MapIterationNext { .. })
            })
        {
            return Err(missing());
        }
        if let Some(owner) = exit.roles.owner {
            let [owner_drop] = owner_drops.as_slice() else {
                return Err(missing());
            };
            let (proof, _, _) = e.body_owner.ok_or_else(missing)?;
            let point = match exit.kind {
                IterationExitKind::Break(expression) | IterationExitKind::Return(expression) => {
                    DropPoint::ControlTransfer(expression)
                }
                _ => return Err(missing()),
            };
            let fact = proof.at(point);
            if fact.is_none()
                || *owner_drop <= *source_end
                || body[*owner_drop].origin != Origin::Source(e.parameter_origin)
                || body[*owner_drop].operation != (Operation::Drop { owner })
            {
                return Err(missing());
            }
        } else if !owner_drops.is_empty() {
            return Err(missing());
        }
        match exit.kind {
            IterationExitKind::Break(_) => {
                let Some(TerminatorKind::Branch(edge)) = terminator(e.function, exit.block) else {
                    return Err(missing());
                };
                if edge.target != e.continuation || exit.returned.is_some() {
                    return Err(missing());
                }
            }
            IterationExitKind::Return(_) => {
                let returned = exit.returned.ok_or_else(missing)?;
                let Some(TerminatorKind::Return { values }) = terminator(e.function, exit.block)
                else {
                    return Err(missing());
                };
                let call = definition(e.function, EntityId::Value(returned)).ok_or_else(missing)?;
                let call_position = body
                    .iter()
                    .position(|instruction| instruction.id == call.id)
                    .ok_or_else(missing)?;
                let Operation::DirectCall {
                    receiver: None,
                    ref arguments,
                    ..
                } = call.operation
                else {
                    return Err(missing());
                };
                if values.as_slice() != [returned]
                    || call.block != exit.block
                    || call.origin
                        != Origin::Source(shape.arms[2].operand_span.ok_or_else(missing)?)
                    || call_position >= exit.cleanup_start
                    || arguments.len() != 2
                {
                    return Err(missing());
                }
                for (argument, source) in arguments.iter().zip([exit.roles.key, exit.roles.value]) {
                    let EntityId::Loan(loan) = *argument else {
                        return Err(missing());
                    };
                    if loan == source {
                        continue;
                    }
                    let reborrow =
                        definition(e.function, EntityId::Loan(loan)).ok_or_else(missing)?;
                    if reborrow.block != exit.block
                        || reborrow.operation != (Operation::SharedReborrow { source })
                        || position(Operation::BorrowEnd { loan })? >= exit.cleanup_start
                        || position(Operation::BorrowEnd { loan })? <= call_position
                    {
                        return Err(missing());
                    }
                }
            }
            _ => return Err(missing()),
        }
    } else {
        let next = exit.next.ok_or_else(missing)?;
        let instruction = definition(e.function, EntityId::Value(next)).ok_or_else(missing)?;
        let next_position = body
            .iter()
            .position(|actual| actual.id == instruction.id)
            .ok_or_else(missing)?;
        if instruction.block != exit.block
            || instruction.operation
                != (Operation::MapIterationNext {
                    source: exit.roles.source,
                    cursor: exit.roles.cursor,
                })
            || next_position <= key_end
            || !source_ends.is_empty()
            || !owner_drops.is_empty()
            || exit.returned.is_some()
        {
            return Err(missing());
        }
        if body
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::MapIterationNext { .. })
            })
            .count()
            != 1
        {
            return Err(missing());
        }
        let Some(TerminatorKind::Branch(edge)) = terminator(e.function, exit.block) else {
            return Err(missing());
        };
        if edge.target != e.blocks[1]
            || edge.arguments.get(e.source_slot) != Some(&EntityId::Loan(exit.roles.source))
            || edge.arguments.get(e.cursor_slot) != Some(&EntityId::Value(next))
        {
            return Err(missing());
        }
        if let Some((_, symbol, _)) = e.body_owner {
            let (slot, _) = unique(
                e.carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, slot)| slot.symbol == Some(symbol)),
            )
            .ok_or_else(missing)?;
            if edge.arguments.get(slot) != exit.roles.owner.map(EntityId::Value).as_ref() {
                return Err(missing());
            }
        }
    }
    Ok(())
}
