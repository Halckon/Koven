//! Current owners traverse actual mutation results and every ordinary CFG incoming.
use super::*;
use crate::ssa::map_source::Formation;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check(
    lowerer: &ExpressionLowerer<'_>,
    formation: &Formation<SymbolId>,
    current: ValueId,
    anchor: ValueId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let function = &lowerer.function;
    let anchor = EntityId::Value(anchor);
    let ty = function.entity(anchor).ok_or_else(bad)?.ty;
    let mut updates = BTreeMap::new();
    for update in &formation.updates {
        let at = lowerer.expression_span(update.expression)?;
        let put = lowerer.typed.map_put(update.expression);
        let remove = lowerer.typed.map_remove(update.expression);
        if put.is_some() == remove.is_some()
            || put
                .map(|d| d.receiver())
                .or_else(|| remove.map(|d| d.receiver()))
                != Some(update.receiver)
        {
            return Err(bad());
        }
        let instruction = unique(function.instructions.iter().filter(|i| {
            i.origin == Origin::Source(at)
                && matches!(
                    i.operation,
                    Operation::MapPut { .. } | Operation::MapRemove { .. }
                )
        }))
        .ok_or_else(bad)?;
        let owner = match instruction.operation {
            Operation::MapPut { owner, .. } if put.is_some() => owner,
            Operation::MapRemove { owner, .. } if remove.is_some() => owner,
            _ => return Err(bad()),
        };
        let Some(&result @ EntityId::Value(_)) = instruction.results.first() else {
            return Err(bad());
        };
        if updates.insert(result, EntityId::Value(owner)).is_some() {
            return Err(bad());
        }
    }
    let mut pending = vec![EntityId::Value(current)];
    let mut dependencies = BTreeMap::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor || dependencies.contains_key(&entity) {
            continue;
        }
        let data = function.entity(entity).ok_or_else(bad)?;
        if data.ty != ty {
            return Err(bad());
        }
        let sources = if let Some(&owner) = updates.get(&entity) {
            vec![owner]
        } else {
            let Definition::BlockParameter { block, index } = data.definition else {
                return Err(bad());
            };
            let parameters = &function.block(block).ok_or_else(bad)?.parameters;
            let predecessors = nested::incoming(function, block);
            if predecessors.is_empty() || parameters.get(index) != Some(&entity) {
                return Err(bad());
            }
            predecessors
                .iter()
                .map(|(_, edge)| {
                    if edge.arguments.len() != parameters.len() {
                        return Err(bad());
                    }
                    edge.arguments.get(index).copied().ok_or_else(bad)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        if sources
            .iter()
            .any(|s| function.entity(*s).map(|e| e.ty) != Some(ty))
        {
            return Err(bad());
        }
        pending.extend(sources.iter().copied());
        dependencies.insert(entity, sources);
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
        for (&entity, sources) in &dependencies {
            if sources.iter().any(|s| anchored.contains(s)) {
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
