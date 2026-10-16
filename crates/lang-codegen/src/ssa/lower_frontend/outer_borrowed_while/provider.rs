//! Actual provider and nullable-branch births; no loan is identified by type alone.
use super::carrier::{
    Columns, Guard, forward, incoming, instructions, parameter, result, transport,
};
use super::*;
use crate::ssa::model::{BinaryOperator, Instruction, SsaTypeKind};
pub(super) struct Provider {
    pub(super) preheader: BlockId,
    pub(super) header: Columns,
    pub(super) body: Columns,
    pub(super) exhausted: Columns,
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
pub(super) fn context<'a>(
    lowerer: &'a ExpressionLowerer<'_>,
    relation: &Relation,
) -> Option<&'a loop_control::LoopContext> {
    let [outer, rest @ ..] = lowerer.loops.as_slice() else {
        return None;
    };
    if rest.len() > 1
        || rest.iter().any(|context| {
            context.for_loop.is_some()
                || context.numeric_loop.is_some()
                || context.outer_borrowed_while.as_ref() != Some(relation)
        })
        || outer.numeric_loop.is_some()
        || outer.for_loop.as_ref()?.statement != relation.outer
        || !outer.entry_views.is_empty()
        || !outer.breaks.is_empty()
        || !outer.continues.is_empty()
    {
        return None;
    }
    Some(outer)
}
pub(super) fn shape(lowerer: &ExpressionLowerer<'_>, relation: &Relation) -> Option<Provider> {
    let function = &*lowerer.function;
    let context = context(lowerer, relation)?;
    let data = context.for_loop.as_ref()?;
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(relation.outer)
        .ok()?
        .span();
    let descriptor = lowerer.owned.iteration(relation.outer)?.descriptor();
    let source_type = *lowerer.type_ids.get(&descriptor.source_type())?;
    let element_type = *lowerer.type_ids.get(&descriptor.element_type())?;
    let source = data.new_source_loan?;
    let begin = result(function, EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return None;
    };
    let root = result(function, EntityId::Place(place))?;
    let Operation::RootPlace { owner: items } = root.operation else {
        return None;
    };
    let preheader = root.block;
    let binding = |symbol| {
        unique(
            context
                .carried
                .slots
                .iter()
                .filter(|slot| slot.symbol == Some(symbol)),
        )
    };
    let current = value(binding(relation.root)?.source)?;
    let construct = result(function, EntityId::Value(items))?;
    if data.is_carried_source
        || binding(relation.items)?.source != EntityId::Value(items)
        || current == items
        || parameter(function, EntityId::Value(current))?
            != (function.entry_block()?, relation.parameter_slot)
        || !matches!(construct.operation, Operation::ContainerConstruct { container, .. } if container == source_type)
        || function.entity(EntityId::Value(items))?.ty != EntityType::Value(source_type)
        || construct.origin
            != Origin::Source(
                lowerer
                    .parsed
                    .ast()
                    .expressions()
                    .get(relation.initializer)
                    .ok()?
                    .span(),
            )
        || begin.block != preheader
        || begin.origin != Origin::Source(span)
        || root.origin != Origin::Source(span)
        || function.entity(EntityId::Place(place))?.ty != EntityType::Place(source_type)
        || function.entity(EntityId::Loan(source))?.ty
            != (EntityType::Loan {
                kind: LoanKind::Shared,
                target: source_type,
            })
        || function.entity(EntityId::Value(current))?.ty
            != EntityType::Value(*lowerer.type_ids.get(&relation.storage)?)
        || lowerer.ssa_types.get(element_type.index())
            != Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
        || !matches!(lowerer.ssa_types.get(source_type.index()), Some(SsaTypeKind::SequentialContainer { kind: crate::ssa::model::SequentialContainerKind::List, element }) if *element == element_type)
    {
        return None;
    }
    let ops = instructions(function, preheader)?;
    let position = |instruction: &Instruction| ops.iter().position(|op| op.id == instruction.id);
    if position(root)? >= position(begin)? {
        return None;
    }
    let length = unique(ops.iter().copied().filter(|instruction| {
        instruction.operation
            == (Operation::ContainerLength {
                owner: EntityId::Loan(source),
            })
    }))?;
    let constant = |number| {
        unique(ops.iter().copied().filter(|instruction| {
            instruction.origin == Origin::Source(span)
                && instruction.operation == Operation::Constant(ScalarConstant::Integer(number))
        }))
    };
    let zero = constant(0)?;
    let one = constant(1)?;
    let step = value(only_result(one)?)?;
    if position(begin)? >= position(length)? || length.origin != Origin::Source(span) {
        return None;
    }
    let term = function.block(preheader)?.terminator.as_ref()?;
    let TerminatorKind::Branch(entry) = &term.kind else {
        return None;
    };
    if term.origin != Origin::Source(span) || entry.target != context.header {
        return None;
    }
    let header = forward(
        function,
        entry,
        &[
            EntityId::Value(current),
            EntityId::Value(items),
            EntityId::Loan(source),
        ],
    )?;
    let numbers = forward(function, entry, &[only_result(length)?, only_result(zero)?])?;
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
        || compare.origin != Origin::Source(span)
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
    let body_source = loan(body.entities[2])?;
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
    if data.provider_header.block != header.block
        || Some(data.provider_header.source) != loan(header.entities[2])
        || data.provider_header.length != length
        || data.provider_header.cursor != cursor
        || data.provider_header.step != step
        || parameter(function, header.entities[2])?.1 != data.source_slot
        || data.source_slot != data.provider_header.source_slot
        || parameter(function, EntityId::Value(length))?.1 != data.length_slot
        || data.length_slot != data.provider_header.length_slot
        || parameter(function, EntityId::Value(cursor))?.1 != data.cursor_slot
        || data.cursor_slot != data.provider_header.cursor_slot
        || data.guarded_element.body() != body.block
        || data.guarded_element.header() != header.block
        || data.guarded_element.source() != body_source
        || data.guarded_element.cursor() != body_cursor
        || data.body_cursor != body_cursor
        || data.guarded_element.step() != step
    {
        return None;
    }
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
        preheader,
        header,
        body,
        exhausted,
        length,
        cursor,
        body_cursor,
        step,
    })
}

/// The only NullableBranch which transport may cross. Its two owner/provider
/// columns are explicit; only the non-null target owns the newly born view.
pub(super) fn guard(
    lowerer: &ExpressionLowerer<'_>,
    relation: &Relation,
    provider: &Provider,
) -> Option<Guard> {
    let function = &*lowerer.function;
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(relation.inner)
        .ok()?
        .span();
    let (block, owner, view, when_null, when_non_null) =
        unique(function.blocks.iter().filter_map(|block| {
            let term = block.terminator.as_ref()?;
            let TerminatorKind::NullableBranch {
                owner,
                view,
                when_null,
                when_non_null,
            } = &term.kind
            else {
                return None;
            };
            (term.origin == Origin::Source(span)).then_some((
                block.id,
                *owner,
                *view,
                when_null,
                when_non_null,
            ))
        }))?;
    let (body_parent, edge) = unique(incoming(function, block).into_iter())?;
    if body_parent != provider.body.block
        || forward(function, edge, &provider.body.entities)?.entities[0] != EntityId::Value(owner)
    {
        return None;
    }
    let header = forward(function, edge, &provider.body.entities)?;
    if !transport(function, header.clone(), &provider.body, None, None) {
        return None;
    }
    let yes = forward(function, when_non_null, &header.entities)?;
    let no = forward(function, when_null, &header.entities)?;
    let target = function.block(yes.block)?;
    let storage = *lowerer.type_ids.get(&relation.storage)?;
    let SsaTypeKind::NullableHandle { inner } = lowerer.ssa_types.get(storage.index())? else {
        return None;
    };
    if no.block == yes.block
        || target.parameters.last() != Some(&EntityId::Loan(view))
        || target.parameters.len() != when_non_null.arguments.len() + 1
        || when_null.arguments.len() != function.block(no.block)?.parameters.len()
        || when_non_null.arguments.contains(&EntityId::Loan(view))
        || when_null.arguments.contains(&EntityId::Loan(view))
        || function.entity(EntityId::Loan(view))?.ty
            != (EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner,
            })
        || incoming(function, yes.block).as_slice() != [(block, when_non_null)]
        || incoming(function, no.block).as_slice() != [(block, when_null)]
        || !matches!(
            lowerer.ssa_types.get(inner.index()),
            Some(SsaTypeKind::HeapOwner { .. })
        )
    {
        return None;
    }
    Some(Guard {
        header: block,
        owner,
        view,
        when_null: no.block,
        when_non_null: yes.block,
    })
}

pub(super) fn completed_backedge(
    lowerer: &ExpressionLowerer<'_>,
    relation: &Relation,
    provider: &Provider,
) -> Option<()> {
    let function = &*lowerer.function;
    let guard = guard(lowerer, relation, provider)?;
    let edges = incoming(function, provider.header.block);
    if edges.len() != 2
        || edges
            .iter()
            .filter(|(block, _)| *block == provider.preheader)
            .count()
            != 1
    {
        return None;
    }
    let (block, edge) = unique(
        edges
            .into_iter()
            .filter(|(block, _)| *block != provider.preheader),
    )?;
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(relation.outer)
        .ok()?
        .span();
    let term = function.block(block)?.terminator.as_ref()?;
    if term.origin != Origin::Source(span)
        || !matches!(&term.kind, TerminatorKind::Branch(actual) if actual == edge)
    {
        return None;
    }
    let ops = instructions(function, block)?;
    let end = unique(
        ops.iter()
            .copied()
            .filter(|op| matches!(op.operation, Operation::BorrowEnd { .. })),
    )?;
    let Operation::BorrowEnd { loan: element } = end.operation else {
        return None;
    };
    let mut entities = provider
        .header
        .entities
        .iter()
        .map(|entity| edge.arguments.get(parameter(function, *entity)?.1).copied())
        .collect::<Option<Vec<_>>>()?;
    entities.push(EntityId::Loan(element));
    if end.origin != Origin::Source(span)
        || edge.arguments.contains(&EntityId::Loan(element))
        || edge.arguments.contains(&EntityId::Loan(guard.view))
        || !transport(
            function,
            Columns { block, entities },
            &provider.body,
            Some((block, element)),
            Some(&guard),
        )
    {
        return None;
    }
    let length = parameter(function, EntityId::Value(provider.length))?.1;
    let cursor = parameter(function, EntityId::Value(provider.cursor))?.1;
    let next = result(function, *edge.arguments.get(cursor)?)?;
    if edge.arguments.get(length) != Some(&EntityId::Value(provider.length))
        || next.block != block
        || next.origin != Origin::Source(span)
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
    // The inner true path ends exactly its new proof before reaching the merge.
    let yes = instructions(function, guard.when_non_null)?;
    let end_view = unique(
        yes.iter()
            .copied()
            .filter(|op| matches!(op.operation, Operation::BorrowEnd { .. })),
    )?;
    let break_span = lowerer
        .parsed
        .ast()
        .expressions()
        .get(relation.breaking)
        .ok()?
        .span();
    if end_view.operation != (Operation::BorrowEnd { loan: guard.view })
        || end_view.origin != Origin::Source(break_span)
        || instructions(function, guard.when_null)?.iter().any(|op| {
            matches!(
                op.operation,
                Operation::BorrowEnd { .. } | Operation::Drop { .. }
            )
        })
    {
        return None;
    }
    Some(())
}
