//! Read-only simultaneous CFG-column checks, grounded in the actual outer branch.
use super::*;
use crate::ssa::model::{Definition, Instruction, SsaTypeKind};
use std::collections::BTreeSet;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Columns {
    pub(super) block: BlockId,
    pub(super) entities: Vec<EntityId>,
}

pub(super) struct Birth {
    pub(super) pair: Columns,
    pub(super) source_owner: ValueId,
}

pub(super) fn parameter(function: &Function, entity: EntityId) -> Option<(BlockId, usize)> {
    let Definition::BlockParameter { block, index } = function.entity(entity)?.definition else {
        return None;
    };
    (function.block(block)?.parameters.get(index) == Some(&entity)).then_some((block, index))
}

pub(super) fn result(function: &Function, entity: EntityId) -> Option<&Instruction> {
    let Definition::InstructionResult { instruction, index } = function.entity(entity)?.definition
    else {
        return None;
    };
    let instruction = function.instruction(instruction)?;
    (instruction.results.get(index) == Some(&entity)).then_some(instruction)
}

pub(super) fn instructions(function: &Function, block: BlockId) -> Option<Vec<&Instruction>> {
    function
        .block(block)?
        .instructions
        .iter()
        .map(|id| function.instruction(*id))
        .collect()
}

pub(super) fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
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
            edges
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

pub(super) fn forward(function: &Function, edge: &Edge, entities: &[EntityId]) -> Option<Columns> {
    let parameters = &function.block(edge.target)?.parameters;
    let entities = entities
        .iter()
        .map(|entity| {
            let index = unique(
                edge.arguments
                    .iter()
                    .enumerate()
                    .filter_map(|(index, value)| (value == entity).then_some(index)),
            )?;
            let target = *parameters.get(index)?;
            (function.entity(target)?.ty == function.entity(*entity)?.ty).then_some(target)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Columns {
        block: edge.target,
        entities,
    })
}

pub(super) fn birth(
    lowerer: &ExpressionLowerer<'_>,
    plan: &Plan,
    proof: NonNullProof,
) -> Option<Birth> {
    let (statement, view) = proof.while_origin()?;
    let [outer, rest @ ..] = lowerer.loops.as_slice() else {
        return None;
    };
    if statement != plan.outer
        || rest.len() > 1
        || rest.iter().any(|context| {
            context.numeric_loop.is_some()
                || context
                    .for_loop
                    .as_ref()
                    .is_none_or(|data| data.statement != plan.statement)
        })
        || outer.for_loop.is_some()
        || outer.numeric_loop.is_some()
        || !outer.entry_views.is_empty()
        || !outer.breaks.is_empty()
        || !outer.continues.is_empty()
    {
        return None;
    }
    let term = lowerer.function.block(outer.header)?.terminator.as_ref()?;
    let TerminatorKind::NullableBranch {
        owner,
        view: actual_view,
        when_non_null,
        when_null,
    } = &term.kind
    else {
        return None;
    };
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(statement)
        .ok()?
        .span();
    let (block, owner_column) = parameter(lowerer.function, EntityId::Value(*owner))?;
    let source_column = unique(
        outer
            .carried
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| (slot.symbol == Some(plan.source_root)).then_some(index)),
    )?;
    let slot = outer.carried.slots.get(owner_column)?;
    if *actual_view != view
        || term.origin != Origin::Source(span)
        || when_non_null.target == when_null.target
        || incoming(lowerer.function, when_non_null.target).as_slice()
            != [(outer.header, when_non_null)]
        || block != outer.header
        || slot.symbol != Some(plan.root)
        || outer
            .carried
            .slots
            .iter()
            .filter(|slot| slot.symbol == Some(plan.root))
            .count()
            != 1
        || when_non_null.arguments.get(owner_column) != Some(&EntityId::Value(*owner))
        || source_column == owner_column
    {
        return None;
    }
    let header_source = *lowerer
        .function
        .block(outer.header)?
        .parameters
        .get(source_column)?;
    if when_non_null.arguments.get(source_column) != Some(&header_source) {
        return None;
    }
    let target = lowerer.function.block(when_non_null.target)?;
    let EntityId::Value(owner) = *target.parameters.get(owner_column)? else {
        return None;
    };
    let EntityId::Value(source_owner) = *target.parameters.get(source_column)? else {
        return None;
    };
    let source_type = *lowerer
        .type_ids
        .get(&lowerer.typed.symbol_type(plan.source_root)?)?;
    let nullable = *lowerer.type_ids.get(&plan.storage)?;
    if owner == source_owner
        || lowerer.ssa_types.get(nullable.index())
            != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
        || !matches!(
            lowerer.ssa_types.get(proof.inner.index()),
            Some(SsaTypeKind::HeapOwner { .. })
        )
        || lowerer.function.entity(EntityId::Value(owner))?.ty != EntityType::Value(nullable)
        || lowerer.function.entity(EntityId::Value(source_owner))?.ty
            != EntityType::Value(source_type)
        || lowerer.function.entity(EntityId::Loan(view))?.ty
            != (EntityType::Loan {
                kind: LoanKind::Shared,
                target: proof.inner,
            })
        || parameter(lowerer.function, EntityId::Loan(view))?.0 != target.id
    {
        return None;
    }
    Some(Birth {
        pair: Columns {
            block: target.id,
            entities: vec![EntityId::Value(owner), EntityId::Loan(view)],
        },
        source_owner,
    })
}

/// All columns cross each predecessor together. Closed ungrounded SCCs fail.
/// Only the caller's exact terminal element end may consume a tracked loan.
pub(super) fn transport(
    function: &Function,
    current: Columns,
    anchor: &Columns,
    allowed_end: Option<(BlockId, LoanId)>,
) -> bool {
    let mut graph = BTreeMap::<Columns, Vec<Columns>>::new();
    let mut pending = vec![current];
    while let Some(state) = pending.pop() {
        if graph.contains_key(&state) {
            continue;
        }
        if state.entities.len() != anchor.entities.len()
            || state.entities.iter().enumerate().any(|(index, entity)| {
                state.entities[..index].contains(entity)
                    || function.entity(*entity).map(|data| data.ty)
                        != function.entity(anchor.entities[index]).map(|data| data.ty)
            })
        {
            return false;
        }
        let Some(ops) = instructions(function, state.block) else {
            return false;
        };
        for instruction in ops {
            match instruction.operation {
                Operation::BorrowEnd { loan }
                    if state.entities.contains(&EntityId::Loan(loan))
                        && allowed_end != Some((state.block, loan)) =>
                {
                    return false;
                }
                Operation::Drop { owner } if state.entities.contains(&EntityId::Value(owner)) => {
                    return false;
                }
                _ => {}
            }
        }
        if state == *anchor {
            graph.insert(state, Vec::new());
            continue;
        }
        let Some(columns) = state
            .entities
            .iter()
            .map(|entity| {
                let (block, index) = parameter(function, *entity)?;
                (block == state.block).then_some(index)
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let predecessors = incoming(function, state.block);
        if predecessors.is_empty() {
            return false;
        }
        let mut parents = Vec::new();
        for (block, edge) in predecessors {
            if !matches!(
                function
                    .block(block)
                    .and_then(|block| block.terminator.as_ref())
                    .map(|term| &term.kind),
                Some(TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. })
            ) {
                return false;
            }
            let Some(entities) = columns
                .iter()
                .map(|column| {
                    let entity = *edge.arguments.get(*column)?;
                    (edge
                        .arguments
                        .iter()
                        .filter(|value| **value == entity)
                        .count()
                        == 1)
                        .then_some(entity)
                })
                .collect::<Option<Vec<_>>>()
            else {
                return false;
            };
            let parent = Columns { block, entities };
            parents.push(parent.clone());
            pending.push(parent);
        }
        graph.insert(state, parents);
    }
    if !graph.contains_key(anchor) {
        return false;
    }
    let mut anchored = BTreeSet::from([anchor.clone()]);
    loop {
        let before = anchored.len();
        for (state, parents) in &graph {
            if parents.iter().any(|parent| anchored.contains(parent)) {
                anchored.insert(state.clone());
            }
        }
        if before == anchored.len() {
            return anchored.len() == graph.len();
        }
    }
}
