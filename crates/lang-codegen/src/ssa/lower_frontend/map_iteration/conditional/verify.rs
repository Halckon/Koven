//! Validate every actual body exit, cleanup suffix and current role provenance.
use super::super::carrier::{Evidence, instructions, terminator};
use super::*;
use crate::ssa::model::Definition;
use lang_frontend::ownership_checking::CleanupOwnerValue;
use std::collections::BTreeSet;

pub(in super::super) fn validate(e: Evidence<'_>, state: &State) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, e.span);
    let [preheader, header, body, exhausted] = e.blocks;
    let plan = e
        .owned
        .map_iteration(state.shape.statement)
        .ok_or_else(missing)?;
    let body_parameters = &e.function.block(body).ok_or_else(missing)?.parameters;
    let (source, cursor) =
        super::super::roles(body_parameters, e.source_slot, e.cursor_slot, e.span)?;
    let expected = plan
        .exits()
        .iter()
        .filter(|exit| exit.kind() != IterationExitKind::Exhaustion)
        .collect::<Vec<_>>();
    if state.exits.len() != expected.len() {
        return Err(missing());
    }
    for exit in &state.exits {
        if state
            .exits
            .iter()
            .filter(|other| other.kind == exit.kind)
            .count()
            != 1
            || !expected.iter().any(|expected| expected.kind() == exit.kind)
        {
            return Err(missing());
        }
        for (actual, anchor) in [
            (EntityId::Loan(exit.roles.source), EntityId::Loan(source)),
            (EntityId::Value(exit.roles.cursor), EntityId::Value(cursor)),
            (EntityId::Loan(exit.roles.key), EntityId::Loan(e.key)),
            (EntityId::Loan(exit.roles.value), EntityId::Loan(e.value)),
        ] {
            nested::transport(e.function, actual, anchor, e.span)?;
        }
        match (exit.roles.owner, e.body_owner) {
            (Some(actual), Some((_, identity, anchor))) if identity == state.shape.source => {
                nested::transport(
                    e.function,
                    EntityId::Value(actual),
                    EntityId::Value(anchor),
                    e.span,
                )?
            }
            (None, None) => {}
            _ => return Err(missing()),
        }
        check_exit(e, state, exit)?;
    }
    // The actual graph must have no unrecorded return/backedge/provider advance.
    // Abort is deliberately terminal without cleanup, as in the ordinary emitter.
    let mut pending = vec![body];
    let mut visited = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if !visited.insert(block) {
            continue;
        }
        if state.exits.iter().any(|exit| exit.block == block) {
            continue;
        }
        if [header, exhausted, e.continuation].contains(&block) {
            return Err(missing());
        }
        if instructions(e.function, block)
            .ok_or_else(missing)?
            .iter()
            .any(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::MapIterationNext { .. }
                        | Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                )
            })
        {
            return Err(missing());
        }
        match terminator(e.function, block).ok_or_else(missing)? {
            TerminatorKind::Branch(edge) => pending.push(edge.target),
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => {
                pending.extend([when_true.target, when_false.target]);
            }
            TerminatorKind::NullableBranch {
                when_null,
                when_non_null,
                ..
            } => {
                pending.extend([when_null.target, when_non_null.target]);
            }
            TerminatorKind::Abort => {}
            _ => return Err(missing()),
        }
    }
    if state
        .exits
        .iter()
        .any(|exit| !visited.contains(&exit.block))
    {
        return Err(missing());
    }
    for block in &e.function.blocks {
        let edges = match block.terminator.as_ref().map(|term| &term.kind) {
            Some(TerminatorKind::Branch(edge)) => vec![edge],
            Some(TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            }) => vec![when_true, when_false],
            Some(TerminatorKind::NullableBranch {
                when_null,
                when_non_null,
                ..
            }) => vec![when_null, when_non_null],
            _ => Vec::new(),
        };
        for _edge in edges.into_iter().filter(|edge| edge.target == header) {
            if block.id != preheader
                && !state.exits.iter().any(|exit| {
                    exit.block == block.id
                        && matches!(
                            exit.kind,
                            IterationExitKind::Continue(_) | IterationExitKind::Fallthrough
                        )
                })
            {
                return Err(missing());
            }
        }
    }
    let exhaustion = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
    )
    .ok_or_else(missing)?;
    let parameters = &e.function.block(exhausted).ok_or_else(missing)?.parameters;
    let (source, _) = super::super::roles(parameters, e.source_slot, e.cursor_slot, e.span)?;
    let body = instructions(e.function, exhausted).ok_or_else(missing)?;
    let mut expected = vec![Operation::BorrowEnd { loan: source }];
    if let Some((proof, identity, owner)) = e.owner {
        if identity != state.shape.source {
            return Err(missing());
        }
        nested::transport(e.function, EntityId::Value(owner), e.seed, e.span)?;
        if proof.at_checked(exhaustion.point(), e.owned)?.is_some() {
            expected.push(Operation::Drop { owner });
        }
    } else if !matches!(e.seed, EntityId::Loan(_)) {
        return Err(missing());
    }
    if body.len() != expected.len() || body.iter().zip(&expected).any(|(i, op)| &i.operation != op)
    {
        return Err(missing());
    }
    if e.continuation == exhausted {
        // With one live exit ordinary merge reuses the exhaustion block. Its
        // continuation is still open here; the caller emits subsequent code.
        if state
            .exits
            .iter()
            .any(|exit| matches!(exit.kind, IterationExitKind::Break(_)))
            || terminator(e.function, exhausted).is_some()
        {
            return Err(missing());
        }
    } else if !matches!(terminator(e.function, exhausted), Some(TerminatorKind::Branch(edge))
        if edge.target == e.continuation)
    {
        return Err(missing());
    }
    Ok(())
}

fn check_exit(e: Evidence<'_>, state: &State, exit: &Exit) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, e.span);
    let plan = e
        .owned
        .map_iteration(state.shape.statement)
        .ok_or_else(missing)?;
    let expected = unique(
        plan.exits()
            .iter()
            .filter(|actual| actual.kind() == exit.kind),
    )
    .ok_or_else(missing)?;
    let body = instructions(e.function, exit.block).ok_or_else(missing)?;
    if exit.cleanup_start > body.len() {
        return Err(missing());
    }
    let descriptor = plan.descriptor();
    let source_target = facts::source_target(state.shape.source);
    let mut at = exit.cleanup_start;
    for action in expected.actions() {
        let operation = match action {
            IterationCleanupAction::FinishProvider(statement)
                if *statement == state.shape.statement =>
            {
                continue;
            }
            IterationCleanupAction::EndSource(statement) if *statement == state.shape.statement => {
                Operation::BorrowEnd {
                    loan: exit.roles.source,
                }
            }
            IterationCleanupAction::EndBinding { statement, symbol }
                if *statement == state.shape.statement =>
            {
                let loan = if *symbol == descriptor.key_binding() {
                    exit.roles.key
                } else if *symbol == descriptor.value_binding() {
                    exit.roles.value
                } else {
                    return Err(missing());
                };
                Operation::BorrowEnd { loan }
            }
            IterationCleanupAction::Drop(fact) => {
                let instruction = body.get(at).ok_or_else(missing)?;
                let Operation::Drop { owner } = instruction.operation else {
                    return Err(missing());
                };
                if instruction.origin != Origin::Source(fact.value_origin()) {
                    return Err(missing());
                }
                if fact.target() == source_target {
                    let (proof, _, _) = e.body_owner.ok_or_else(missing)?;
                    if exit.roles.owner != Some(owner)
                        || proof.at_checked(expected.point(), e.owned)? != Some(*fact)
                    {
                        return Err(missing());
                    }
                } else {
                    let Some(CleanupOwnerValue::Expression { origin, .. }) = fact
                        .owner()
                        .and_then(|owner| e.owned.cleanup_conditions().owner_value(owner))
                    else {
                        return Err(missing());
                    };
                    let ty = e
                        .function
                        .entity(EntityId::Value(owner))
                        .ok_or_else(missing)?
                        .ty;
                    let birth = unique(e.function.instructions.iter().filter_map(|instruction| {
                        let [EntityId::Value(birth)] = instruction.results.as_slice() else {
                            return None;
                        };
                        (instruction.origin == Origin::Source(*origin)
                            && e.function
                                .entity(EntityId::Value(*birth))
                                .map(|data| data.ty)
                                == Some(ty))
                        .then_some(*birth)
                    }))
                    .ok_or_else(missing)?;
                    nested::transport(
                        e.function,
                        EntityId::Value(owner),
                        EntityId::Value(birth),
                        e.span,
                    )?;
                }
                Operation::Drop { owner }
            }
            _ => return Err(missing()),
        };
        if body.get(at).map(|instruction| &instruction.operation) != Some(&operation) {
            return Err(missing());
        }
        at += 1;
    }
    if exit_shape::terminal(exit.kind) {
        if exit.next.is_some() || at != body.len() {
            return Err(missing());
        }
        match (exit.kind, terminator(e.function, exit.block)) {
            (IterationExitKind::Break(_), Some(TerminatorKind::Branch(edge)))
                if edge.target == e.continuation && exit.returned.is_none() => {}
            (IterationExitKind::Return(_), Some(TerminatorKind::Return { values })) => {
                if values.as_slice() != exit.returned.as_slice()
                    || values.len() != e.function.return_types.len()
                    || values
                        .iter()
                        .zip(&e.function.return_types)
                        .any(|(value, ty)| {
                            e.function
                                .entity(EntityId::Value(*value))
                                .map(|data| data.ty)
                                != Some(EntityType::Value(*ty))
                        })
                {
                    return Err(missing());
                }
                if let Some(returned) = exit.returned {
                    let entity = EntityId::Value(returned);
                    if let Some(Definition::InstructionResult { instruction, index }) =
                        e.function.entity(entity).map(|data| data.definition)
                    {
                        let producer = e.function.instruction(instruction).ok_or_else(missing)?;
                        if producer.results.get(index) != Some(&entity)
                            || (producer.block == exit.block
                                && body[..exit.cleanup_start]
                                    .iter()
                                    .all(|instruction| instruction.id != producer.id))
                        {
                            return Err(missing());
                        }
                    }
                }
            }
            _ => return Err(missing()),
        }
    } else {
        let next = exit.next.ok_or_else(missing)?;
        if exit.returned.is_some()
            || at + 1 != body.len()
            || body[at].operation
                != (Operation::MapIterationNext {
                    source: exit.roles.source,
                    cursor: exit.roles.cursor,
                })
            || body[at].results != [EntityId::Value(next)]
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
        if let Some((_, identity, _)) = e.body_owner {
            let (column, _) = unique(
                e.carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, slot)| identity.matches_slot(slot)),
            )
            .ok_or_else(missing)?;
            if edge.arguments.get(column).copied() != exit.roles.owner.map(EntityId::Value) {
                return Err(missing());
            }
        }
    }
    if let EntityId::Loan(entry) = e.seed {
        if body
            .iter()
            .any(|instruction| instruction.operation == Operation::BorrowEnd { loan: entry })
        {
            return Err(missing());
        }
    }
    Ok(())
}
