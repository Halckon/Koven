//! Independent entity-role fixed point over actual incoming SSA edges.
use super::invalid;
use crate::ssa::{
    lowering_support::LoweringError,
    model::{BlockId, EntityId, Function, TerminatorKind},
};
use lang_frontend::source::Span;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Role {
    Owner,
    Source,
    Result,
    Flag,
}

/// Blocks are the actual entry, header, body and cleanup exit, in that order.
/// Seeds must be independently established instruction results / entry parameters.
pub(super) fn prove(
    function: &Function,
    blocks: [BlockId; 4],
    seeds: &[(EntityId, Role)],
    span: Span,
) -> Result<BTreeMap<EntityId, Role>, LoweringError> {
    let set = blocks.into_iter().collect::<BTreeSet<_>>();
    if set.len() != 4
        || !(4..=5).contains(&function.blocks.len())
        || blocks.iter().any(|b| function.block(*b).is_none())
        || seeds.len() != 4
        || seeds
            .iter()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            .len()
            != 4
        || seeds
            .iter()
            .map(|(_, role)| role)
            .collect::<BTreeSet<_>>()
            .len()
            != 4
    {
        return Err(invalid(span));
    }
    let mut roles = seeds.iter().copied().collect::<BTreeMap<_, _>>();
    for (id, role) in seeds {
        let kind = matches!(
            (id, role),
            (EntityId::Value(_), Role::Owner)
                | (EntityId::Loan(_), Role::Source | Role::Result | Role::Flag)
        );
        let defined = function.blocks.iter().any(|b| b.parameters.contains(id))
            || function.instructions.iter().any(|i| i.results.contains(id));
        if !kind || !defined {
            return Err(invalid(span));
        }
    }
    let mut incoming = BTreeMap::<BlockId, Vec<(BlockId, Vec<EntityId>)>>::new();
    for block in &function.blocks {
        let term = &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind;
        let edges = match term {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::Return { values } if values.is_empty() => Vec::new(),
            _ => return Err(invalid(span)),
        };
        for edge in edges {
            let target = function.block(edge.target).ok_or_else(|| invalid(span))?;
            if target.parameters.len() != edge.arguments.len() {
                return Err(invalid(span));
            }
            incoming
                .entry(edge.target)
                .or_default()
                .push((block.id, edge.arguments.clone()));
        }
    }
    // Exactly one entry edge and one backedge feed header; no hidden predecessor.
    let expected = [
        (blocks[1], vec![blocks[0], blocks[2]]),
        (blocks[2], vec![blocks[1]]),
    ];
    if incoming.contains_key(&blocks[0]) {
        return Err(invalid(span));
    }
    for (target, predecessors) in expected {
        let actual = incoming.get(&target).ok_or_else(|| invalid(span))?;
        if actual.len() != predecessors.len()
            || predecessors
                .iter()
                .any(|p| actual.iter().filter(|(b, _)| b == p).count() != 1)
        {
            return Err(invalid(span));
        }
    }
    let extra = function
        .blocks
        .iter()
        .filter(|b| !set.contains(&b.id))
        .collect::<Vec<_>>();
    let exit_predecessor = if let [extra] = extra.as_slice() {
        let incoming_extra = incoming.get(&extra.id).ok_or_else(|| invalid(span))?;
        if incoming_extra.len() != 1 || incoming_extra[0].0 != blocks[1] {
            return Err(invalid(span));
        }
        extra.id
    } else {
        blocks[1]
    };
    let exit_edges = incoming.get(&blocks[3]).ok_or_else(|| invalid(span))?;
    if exit_edges.len() != 1 || exit_edges[0].0 != exit_predecessor {
        return Err(invalid(span));
    }
    // Known entry seeds initiate the cycle. Missing backedge facts are checked only
    // after convergence; a known incompatible incoming role fails immediately.
    loop {
        let mut changed = false;
        for (target, edges) in &incoming {
            let parameters = &function
                .block(*target)
                .ok_or_else(|| invalid(span))?
                .parameters;
            for (_, arguments) in edges {
                for (parameter, argument) in parameters.iter().zip(arguments) {
                    let Some(role) = roles.get(argument).copied() else {
                        continue;
                    };
                    match roles.get(parameter) {
                        Some(previous) if *previous != role => return Err(invalid(span)),
                        Some(_) => {}
                        None => {
                            roles.insert(*parameter, role);
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for (index, block) in blocks.iter().enumerate().skip(1) {
        let parameters = &function
            .block(*block)
            .ok_or_else(|| invalid(span))?
            .parameters;
        if parameters.iter().any(|p| !roles.contains_key(p)) {
            return Err(invalid(span));
        }
        for role in [Role::Owner, Role::Source, Role::Result, Role::Flag] {
            let count = parameters
                .iter()
                .filter(|p| roles.get(p) == Some(&role))
                .count();
            if if index == 3 && role == Role::Flag {
                count > 1
            } else {
                count != 1
            } {
                return Err(invalid(span));
            }
        }
        for (_, arguments) in incoming.get(block).ok_or_else(|| invalid(span))? {
            for (parameter, argument) in parameters.iter().zip(arguments) {
                if roles.get(argument) != roles.get(parameter) {
                    return Err(invalid(span));
                }
            }
        }
    }
    for block in &function.blocks {
        if block.parameters.iter().any(|p| !roles.contains_key(p)) {
            return Err(invalid(span));
        }
        if block.id == blocks[0] {
            continue;
        }
        for role in [Role::Owner, Role::Source, Role::Result, Role::Flag] {
            let count = block
                .parameters
                .iter()
                .filter(|p| roles.get(p) == Some(&role))
                .count();
            if (role == Role::Flag && count > 1) || (role != Role::Flag && count != 1) {
                return Err(invalid(span));
            }
        }
        for (_, arguments) in incoming.get(&block.id).ok_or_else(|| invalid(span))? {
            for (parameter, argument) in block.parameters.iter().zip(arguments) {
                if roles.get(argument) != roles.get(parameter) {
                    return Err(invalid(span));
                }
            }
        }
    }
    // A role label alone is insufficient: a predecessor must transport its own
    // live parameter, not an older dominating entity with the same semantic role.
    for (target, edges) in &incoming {
        let target_parameters = &function
            .block(*target)
            .ok_or_else(|| invalid(span))?
            .parameters;
        for (predecessor, arguments) in edges {
            let local = if *predecessor == blocks[0] {
                seeds
                    .iter()
                    .map(|(entity, role)| (*role, *entity))
                    .collect::<BTreeMap<_, _>>()
            } else {
                function
                    .block(*predecessor)
                    .ok_or_else(|| invalid(span))?
                    .parameters
                    .iter()
                    .map(|entity| roles.get(entity).map(|role| (*role, *entity)))
                    .collect::<Option<BTreeMap<_, _>>>()
                    .ok_or_else(|| invalid(span))?
            };
            for (argument, parameter) in arguments.iter().zip(target_parameters) {
                let role = roles.get(parameter).ok_or_else(|| invalid(span))?;
                if local.get(role) != Some(argument) {
                    return Err(invalid(span));
                }
            }
        }
    }
    Ok(roles)
}
