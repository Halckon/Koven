//! Complete published owner incoming graph, rooted at this actual owned parameter.
use super::*;
use lang_frontend::ownership_checking::{CleanupConditionId, CleanupOwnerValueId};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn parameter(
    owned: &OwnershipCheckedFile,
    symbol: SymbolId,
    origin: Span,
    owner: CleanupOwnerValueId,
    span: Span,
) -> Result<CleanupConditionId, LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let conditions = owned.cleanup_conditions();
    let mut pending = vec![owner];
    let mut parents = BTreeMap::new();
    let mut available = BTreeMap::new();
    let mut seeds = BTreeSet::new();
    let mut values = Vec::new();
    while let Some(current) = pending.pop() {
        if available.contains_key(&current) {
            continue;
        }
        match conditions.owner_value(current).ok_or_else(bad)? {
            CleanupOwnerValue::Parameter {
                symbol: actual,
                origin: at,
            } if *actual == symbol && *at == origin => {
                // The public Entry binding publishes the Always ID; do not invent one by index.
                let entry = owned
                    .map_iterations()
                    .iter()
                    .flat_map(|p| p.closure_phi_incomings())
                    .flat_map(|i| i.bindings())
                    .flat_map(|b| b.values())
                    .find(|v| v.source() == current)
                    .ok_or_else(bad)?;
                if conditions.get(entry.condition()) != Some(&CleanupCondition::Always) {
                    return Err(bad());
                }
                available.insert(current, entry.condition());
                seeds.insert(current);
            }
            CleanupOwnerValue::IterationPhi {
                statement,
                boundary,
                symbol: actual,
                ..
            } if *actual == symbol => {
                let plan = owned.map_iteration(*statement).ok_or_else(bad)?;
                let phi = super::phi(
                    conditions,
                    plan,
                    symbol,
                    *boundary,
                    conditions
                        .owner_value(current)
                        .and_then(|o| match o {
                            CleanupOwnerValue::IterationPhi { origin, .. } => Some(*origin),
                            _ => None,
                        })
                        .ok_or_else(bad)?,
                )?;
                if phi.owner() != current
                    || plan.closure_phis().len() != 2
                    || !plan.capture_graph().nodes().is_empty()
                {
                    return Err(unsupported());
                }
                available.insert(current, phi.availability_condition());
                let mut inputs = Vec::new();
                let mut count = 0;
                for incoming in plan
                    .closure_phi_incomings()
                    .iter()
                    .filter(|i| i.boundary() == *boundary)
                {
                    let kind = incoming.kind();
                    let point = match kind {
                        IterationPhiIncomingKind::Entry => {
                            DropPoint::AfterExpression(plan.descriptor().source())
                        }
                        IterationPhiIncomingKind::Fallthrough => unique(
                            plan.exits()
                                .iter()
                                .filter(|e| e.kind() == IterationExitKind::Fallthrough),
                        )
                        .ok_or_else(bad)?
                        .point(),
                        IterationPhiIncomingKind::Continue(e)
                        | IterationPhiIncomingKind::Break(e) => DropPoint::ControlTransfer(e),
                        IterationPhiIncomingKind::Exhaustion => DropPoint::LoopExit(*statement),
                    };
                    let binding = super::incoming(conditions, plan, phi, kind, point, span)?;
                    count += 1;
                    match binding.values() {
                        [] => return Err(unsupported()),
                        [value] if value.condition() == binding.available_when() => {
                            let header = super::phi(
                                conditions,
                                plan,
                                symbol,
                                IterationPhiBoundary::Header,
                                match conditions.owner_value(current) {
                                    Some(CleanupOwnerValue::IterationPhi { origin, .. }) => *origin,
                                    _ => return Err(bad()),
                                },
                            )?;
                            if kind != IterationPhiIncomingKind::Entry
                                && value.source() != header.owner()
                            {
                                return Err(bad());
                            }
                            values.push((value.source(), value.condition()));
                            inputs.push(value.source());
                            pending.push(value.source());
                        }
                        _ => return Err(bad()),
                    }
                }
                let expected = if *boundary == IterationPhiBoundary::Header {
                    1 + usize::from(plan.exits().iter().any(|e| {
                        matches!(
                            e.kind(),
                            IterationExitKind::Fallthrough | IterationExitKind::Continue(_)
                        )
                    }))
                } else {
                    1 + usize::from(
                        plan.exits()
                            .iter()
                            .any(|e| matches!(e.kind(), IterationExitKind::Break(_))),
                    )
                };
                if count != expected {
                    return Err(bad());
                }
                parents.insert(current, inputs);
            }
            CleanupOwnerValue::Parameter { .. } => return Err(bad()),
            _ => return Err(unsupported()),
        }
    }
    if values
        .iter()
        .any(|(source, condition)| available.get(source) != Some(condition))
    {
        return Err(bad());
    }
    loop {
        let before = seeds.len();
        for (target, inputs) in &parents {
            if inputs.iter().any(|source| seeds.contains(source)) {
                seeds.insert(*target);
            }
        }
        if seeds.len() == before {
            break;
        }
    }
    if !available.keys().all(|id| seeds.contains(id)) {
        return Err(bad());
    }
    available.get(&owner).copied().ok_or_else(bad)
}
