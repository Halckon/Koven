//! Exact source/element births and the complete single-provider edge schedule.
use super::carrier::{
    Birth, Columns, forward, incoming, instructions, parameter, result, transport,
};
use super::*;
use crate::ssa::model::{BinaryOperator, Instruction, SsaTypeKind};

pub(super) struct Provider {
    pub(super) header: Columns,
    pub(super) body: Columns,
    pub(super) exhausted: Columns,
    pub(super) source: LoanId,
    pub(super) length: ValueId,
    pub(super) cursor: ValueId,
    pub(super) body_cursor: ValueId,
    pub(super) step: ValueId,
}

pub(super) fn value(entity: EntityId) -> Option<ValueId> {
    if let EntityId::Value(value) = entity {
        Some(value)
    } else {
        None
    }
}
pub(super) fn loan(entity: EntityId) -> Option<LoanId> {
    if let EntityId::Loan(loan) = entity {
        Some(loan)
    } else {
        None
    }
}
fn only_result(instruction: &Instruction) -> Option<EntityId> {
    let [entity] = instruction.results.as_slice() else {
        return None;
    };
    Some(*entity)
}

pub(super) fn shape(
    lowerer: &ExpressionLowerer<'_>,
    plan: &Plan,
    birth: &Birth,
) -> Option<Provider> {
    let function = &*lowerer.function;
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(plan.statement)
        .ok()?
        .span();
    let descriptor = lowerer.owned.iteration(plan.statement)?.descriptor();
    let source_type = *lowerer.type_ids.get(&descriptor.source_type())?;
    let element_type = *lowerer.type_ids.get(&descriptor.element_type())?;
    if lowerer.ssa_types.get(element_type.index())
        != Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        })
        || !matches!(lowerer.ssa_types.get(source_type.index()), Some(SsaTypeKind::SequentialContainer {
            kind: crate::ssa::model::SequentialContainerKind::List, element }) if *element == element_type)
    {
        return None;
    }
    let ops = instructions(function, birth.pair.block)?;
    let root = unique(ops.iter().copied().filter(|instruction| {
        matches!(instruction.operation,
        Operation::RootPlace { owner } if owner == birth.source_owner)
    }))?;
    let EntityId::Place(place) = only_result(root)? else {
        return None;
    };
    let begin = unique(
        ops.iter()
            .copied()
            .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. })),
    )?;
    if begin.operation
        != (Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        })
        || root.origin != Origin::Source(span)
        || begin.origin != Origin::Source(span)
        || function.entity(EntityId::Place(place))?.ty != EntityType::Place(source_type)
        || ops.iter().position(|op| op.id == root.id)?
            >= ops.iter().position(|op| op.id == begin.id)?
    {
        return None;
    }
    let source = loan(only_result(begin)?)?;
    if function.entity(EntityId::Loan(source))?.ty
        != (EntityType::Loan {
            kind: LoanKind::Shared,
            target: source_type,
        })
    {
        return None;
    }
    let length = unique(ops.iter().copied().filter(|instruction| {
        instruction.operation
            == (Operation::ContainerLength {
                owner: EntityId::Loan(source),
            })
    }))?;
    let zero = unique(ops.iter().copied().filter(|instruction| {
        instruction.operation == Operation::Constant(ScalarConstant::Integer(0))
    }))?;
    let one = unique(ops.iter().copied().filter(|instruction| {
        instruction.operation == Operation::Constant(ScalarConstant::Integer(1))
    }))?;
    if [length, zero, one]
        .iter()
        .any(|op| op.origin != Origin::Source(span))
    {
        return None;
    }
    let (length_value, zero_value, step) = (
        value(only_result(length)?)?,
        value(only_result(zero)?)?,
        value(only_result(one)?)?,
    );
    let term = function.block(birth.pair.block)?.terminator.as_ref()?;
    let TerminatorKind::Branch(entry) = &term.kind else {
        return None;
    };
    if term.origin != Origin::Source(span) || entry.target == birth.pair.block {
        return None;
    }
    let mut entry_entities = birth.pair.entities.clone();
    entry_entities.extend([EntityId::Value(birth.source_owner), EntityId::Loan(source)]);
    let header = forward(function, entry, &entry_entities)?;
    let numbers = forward(
        function,
        entry,
        &[EntityId::Value(length_value), EntityId::Value(zero_value)],
    )?;
    let (length, cursor) = (value(numbers.entities[0])?, value(numbers.entities[1])?);
    let term = function.block(header.block)?.terminator.as_ref()?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return None;
    };
    let compare = result(function, EntityId::Value(*condition))?;
    if term.origin != Origin::Source(span)
        || compare.block != header.block
        || compare.operation
            != (Operation::Compare {
                operator: ComparisonOperator::LessThan,
                left: cursor,
                right: length,
            })
        || when_true.target == when_false.target
        || when_true.target == header.block
        || when_false.target == header.block
    {
        return None;
    }
    let mut body = forward(function, when_true, &header.entities)?;
    let exhausted = forward(function, when_false, &header.entities)?;
    if incoming(function, body.block).as_slice() != [(header.block, when_true)]
        || incoming(function, exhausted.block).as_slice() != [(header.block, when_false)]
    {
        return None;
    }
    let body_cursor = value(forward(function, when_true, &[EntityId::Value(cursor)])?.entities[0])?;
    let body_source = loan(body.entities[3])?;
    let ops = instructions(function, body.block)?;
    let element_place = unique(ops.iter().copied().filter(|instruction| {
        matches!(
            instruction.operation,
            Operation::ContainerElementPlace { .. }
        )
    }))?;
    let EntityId::Place(place) = only_result(element_place)? else {
        return None;
    };
    let begin = unique(
        ops.iter()
            .copied()
            .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. })),
    )?;
    if element_place.operation
        != (Operation::ContainerElementPlace {
            owner: EntityId::Loan(body_source),
            index: body_cursor,
        })
        || begin.operation
            != (Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            })
        || element_place.origin != Origin::Source(span)
        || begin.origin != Origin::Source(span)
        || function.entity(EntityId::Place(place))?.ty != EntityType::Place(element_type)
        || ops.iter().position(|op| op.id == element_place.id)?
            >= ops.iter().position(|op| op.id == begin.id)?
    {
        return None;
    }
    let element = loan(only_result(begin)?)?;
    if function.entity(EntityId::Loan(element))?.ty
        != (EntityType::Loan {
            kind: LoanKind::Shared,
            target: element_type,
        })
    {
        return None;
    }
    body.entities.push(EntityId::Loan(element));
    for state in [&header, &body, &exhausted] {
        if function.block(state.block)?.origin != Origin::Source(span)
            || function
                .block(state.block)?
                .parameters
                .iter()
                .any(|entity| {
                    matches!(entity, EntityId::Loan(_)) && !state.entities.contains(entity)
                })
        {
            return None;
        }
    }
    Some(Provider {
        header,
        body,
        exhausted,
        source,
        length,
        cursor,
        body_cursor,
        step,
    })
}

pub(super) fn backedges(
    lowerer: &ExpressionLowerer<'_>,
    plan: &Plan,
    birth: &Birth,
    provider: &Provider,
) -> Option<Vec<(LoanId, LoanId)>> {
    let function = &*lowerer.function;
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(plan.statement)
        .ok()?
        .span();
    let iteration = lowerer.owned.iteration(plan.statement)?;
    let mut expected = Vec::new();
    for exit in iteration.exits() {
        match exit.kind() {
            IterationExitKind::Continue(expression) => expected.push(
                lowerer
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .ok()?
                    .span(),
            ),
            IterationExitKind::Fallthrough => expected.push(span),
            IterationExitKind::Exhaustion => {}
            _ => return None,
        }
    }
    let edges = incoming(function, provider.header.block);
    if edges.len() != expected.len() + 1 {
        return None;
    }
    let columns = provider
        .header
        .entities
        .iter()
        .map(|entity| parameter(function, *entity).map(|(_, index)| index))
        .collect::<Option<Vec<_>>>()?;
    let length_column = parameter(function, EntityId::Value(provider.length))?.1;
    let cursor_column = parameter(function, EntityId::Value(provider.cursor))?.1;
    let mut entry_count = 0;
    let mut ended = Vec::new();
    for (block, edge) in edges {
        if block == birth.pair.block {
            entry_count += 1;
            continue;
        }
        let term = function.block(block)?.terminator.as_ref()?;
        let TerminatorKind::Branch(actual_edge) = &term.kind else {
            return None;
        };
        if actual_edge != edge {
            return None;
        }
        let Origin::Source(origin) = &term.origin else {
            return None;
        };
        let origin = *origin;
        let index = expected.iter().position(|span| *span == origin)?;
        expected.remove(index);
        let ops = instructions(function, block)?;
        let end =
            unique(ops.iter().copied().filter(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { .. })
            }))?;
        let Operation::BorrowEnd { loan: element } = end.operation else {
            return None;
        };
        if end.origin != Origin::Source(origin) {
            return None;
        }
        let mut entities = columns
            .iter()
            .map(|column| edge.arguments.get(*column).copied())
            .collect::<Option<Vec<_>>>()?;
        if edge.arguments.contains(&EntityId::Loan(element)) {
            return None;
        }
        ended.push((loan(entities[3])?, element));
        entities.push(EntityId::Loan(element));
        if !transport(
            function,
            Columns { block, entities },
            &provider.body,
            Some((block, element)),
        ) {
            return None;
        }
        if edge.arguments.get(length_column) != Some(&EntityId::Value(provider.length)) {
            return None;
        }
        let next = value(*edge.arguments.get(cursor_column)?)?;
        let next = result(function, EntityId::Value(next))?;
        if next.block != block
            || next.origin != Origin::Source(origin)
            || next.operation
                != (Operation::Binary {
                    operator: BinaryOperator::Add,
                    left: provider.body_cursor,
                    right: provider.step,
                })
            || ops.iter().position(|op| op.id == end.id)?
                >= ops.iter().position(|op| op.id == next.id)?
        {
            return None;
        }
    }
    (entry_count == 1 && expected.is_empty()).then_some(ended)
}

pub(super) fn complete(
    lowerer: &ExpressionLowerer<'_>,
    plan: &Plan,
    birth: &Birth,
    provider: &Provider,
) -> Option<()> {
    backedges(lowerer, plan, birth, provider)?;
    let function = &*lowerer.function;
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(plan.statement)
        .ok()?
        .span();
    if lowerer.block != provider.exhausted.block
        || lowerer.bindings.contains_key(&plan.source_root)
        || lowerer.borrow_bindings.contains_key(&plan.element)
    {
        return None;
    }
    let source = loan(provider.exhausted.entities[3])?;
    let owner = value(provider.exhausted.entities[2])?;
    let ops = instructions(function, provider.exhausted.block)?;
    let end =
        unique(ops.iter().enumerate().filter(|(_, instruction)| {
            matches!(instruction.operation, Operation::BorrowEnd { .. })
        }))?;
    let drop = unique(
        ops.iter()
            .enumerate()
            .filter(|(_, instruction)| matches!(instruction.operation, Operation::Drop { .. })),
    )?;
    if end.0 >= drop.0
        || end.1.operation != (Operation::BorrowEnd { loan: source })
        || end.1.origin != Origin::Source(span)
        || drop.1.operation != (Operation::Drop { owner })
        || drop.1.origin != Origin::Source(plan.source_drop.value_origin())
        || ops.iter().any(|instruction| {
            matches!(
                instruction.operation,
                Operation::BorrowBegin { .. } | Operation::ContainerElementPlace { .. }
            )
        })
        || ops[end.0 + 1..].iter().any(|instruction| {
            instruction
                .operation
                .entities()
                .contains(&EntityId::Loan(source))
        })
        || ops[drop.0 + 1..].iter().any(|instruction| {
            instruction
                .operation
                .entities()
                .contains(&EntityId::Value(owner))
        })
    {
        return None;
    }
    Some(())
}
