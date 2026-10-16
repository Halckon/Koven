//! Simultaneous provider columns, crossing only the relation's one nullable guard.
use super::*;
use crate::ssa::model::{Definition, Instruction};
use std::collections::BTreeSet;
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Columns {
    pub(super) block: BlockId,
    pub(super) entities: Vec<EntityId>,
}
pub(super) struct Guard {
    pub(super) header: BlockId,
    pub(super) owner: ValueId,
    pub(super) view: LoanId,
    pub(super) when_null: BlockId,
    pub(super) when_non_null: BlockId,
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

pub(super) fn transport(
    function: &Function,
    current: Columns,
    anchor: &Columns,
    allowed_end: Option<(BlockId, LoanId)>,
    guard: Option<&Guard>,
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
            match function
                .block(block)
                .and_then(|block| block.terminator.as_ref())
                .map(|term| &term.kind)
            {
                Some(TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. }) => {}
                Some(TerminatorKind::NullableBranch {
                    owner,
                    view,
                    when_null,
                    when_non_null,
                }) if guard.is_some_and(|guard| {
                    guard.header == block
                        && guard.owner == *owner
                        && guard.view == *view
                        && guard.when_null == when_null.target
                        && guard.when_non_null == when_non_null.target
                }) => {}
                _ => return false,
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
