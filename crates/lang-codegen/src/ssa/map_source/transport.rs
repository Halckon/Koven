//! Model-only owner transport through witnessed mutations and every actual CFG incoming.
use super::{LoweringError, LoweringErrorKind};
use crate::ssa::model::{Definition, EntityId, Function, TerminatorKind};
use lang_frontend::source::Span;
use std::collections::{BTreeMap, BTreeSet};

pub(in super::super) fn check(
    function: &Function,
    current: EntityId,
    anchor: EntityId,
    updates: &BTreeMap<EntityId, EntityId>,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(span),
    };
    let ty = function.entity(anchor).ok_or_else(bad)?.ty;
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor || dependencies.contains_key(&entity) {
            continue;
        }
        let data = function.entity(entity).ok_or_else(bad)?;
        if data.ty != ty {
            return Err(bad());
        }
        let parents = if let Some(&input) = updates.get(&entity) {
            vec![input]
        } else {
            let Definition::BlockParameter { block, index } = data.definition else {
                return Err(bad());
            };
            let parameters = &function.block(block).ok_or_else(bad)?.parameters;
            if parameters.get(index) != Some(&entity) {
                return Err(bad());
            }
            let mut parents = Vec::new();
            for predecessor in &function.blocks {
                let edges = match predecessor.terminator.as_ref().map(|t| &t.kind) {
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
                for edge in edges.into_iter().filter(|edge| edge.target == block) {
                    if edge.arguments.len() != parameters.len() {
                        return Err(bad());
                    }
                    parents.push(*edge.arguments.get(index).ok_or_else(bad)?);
                }
            }
            if parents.is_empty() {
                return Err(bad());
            }
            parents
        };
        if parents
            .iter()
            .any(|parent| function.entity(*parent).map(|e| e.ty) != Some(ty))
        {
            return Err(bad());
        }
        pending.extend(parents.iter().copied());
        dependencies.insert(entity, parents);
    }
    if updates
        .keys()
        .any(|value| !dependencies.contains_key(value))
    {
        return Err(bad());
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (&entity, parents) in &dependencies {
            if parents.iter().any(|parent| anchored.contains(parent)) {
                anchored.insert(entity);
            }
        }
        if before == anchored.len() {
            break;
        }
    }
    if dependencies.keys().any(|entity| !anchored.contains(entity)) {
        return Err(bad());
    }
    Ok(())
}
