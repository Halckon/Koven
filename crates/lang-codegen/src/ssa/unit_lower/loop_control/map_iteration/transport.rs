//! Exact CFG parameter transport. Instruction results and root/type aliases are not provenance.
use super::*;
use crate::ssa::model::{Definition, Edge, Function};

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

pub(super) fn parameter(
    function: &Function,
    block: BlockId,
    index: usize,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let parameter = *function
        .block(block)
        .and_then(|block| block.parameters.get(index))
        .ok_or_else(bad)?;
    if function.entity(parameter).map(|data| data.definition)
        != Some(Definition::BlockParameter { block, index })
    {
        return Err(bad());
    }
    Ok(parameter)
}

pub(super) fn along(
    function: &Function,
    edge: &Edge,
    before: EntityId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let block = function.block(edge.target).ok_or_else(bad)?;
    if block.parameters.len() != edge.arguments.len() {
        return Err(bad());
    }
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| (*argument == before).then_some(index))
        .collect::<Vec<_>>();
    let [index] = columns.as_slice() else {
        return Err(bad());
    };
    let after = parameter(function, edge.target, *index, span)?;
    if function.entity(before).map(|data| data.ty) != function.entity(after).map(|data| data.ty) {
        return Err(bad());
    }
    Ok(after)
}

/// Every incoming arm must be an exact typed parameter transport, and every visited
/// parameter must reach the immutable entry. A closed cycle cannot prove itself.
pub(super) fn from_anchor(
    function: &Function,
    current: EntityId,
    anchor: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let ty = function
        .entity(anchor)
        .map(|data| data.ty)
        .ok_or_else(bad)?;
    let mut dependencies = BTreeMap::<EntityId, Vec<EntityId>>::new();
    let mut pending = vec![current];
    while let Some(entity) = pending.pop() {
        if entity == anchor || dependencies.contains_key(&entity) {
            continue;
        }
        let data = function.entity(entity).ok_or_else(bad)?;
        let Definition::BlockParameter { block, index } = data.definition else {
            return Err(bad());
        };
        if data.ty != ty || parameter(function, block, index, span)? != entity {
            return Err(bad());
        }
        let edges = incoming(function, block);
        if edges.is_empty() {
            return Err(bad());
        }
        let mut parents = Vec::new();
        for (_, edge) in edges {
            if edge.arguments.len() != function.block(block).ok_or_else(bad)?.parameters.len() {
                return Err(bad());
            }
            let parent = *edge.arguments.get(index).ok_or_else(bad)?;
            if function.entity(parent).map(|data| data.ty) != Some(ty) {
                return Err(bad());
            }
            parents.push(parent);
            pending.push(parent);
        }
        dependencies.insert(entity, parents);
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let old = anchored.len();
        for (entity, parents) in &dependencies {
            if parents.iter().any(|parent| anchored.contains(parent)) {
                anchored.insert(*entity);
            }
        }
        if anchored.len() == old {
            break;
        }
    }
    if dependencies.keys().any(|entity| !anchored.contains(entity)) {
        return Err(bad());
    }
    Ok(())
}
