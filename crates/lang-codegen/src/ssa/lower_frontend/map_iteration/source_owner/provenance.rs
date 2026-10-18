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
    rooted(owned, symbol, origin, None, owner, span, false)
}

pub(super) fn formed(
    owned: &OwnershipCheckedFile,
    symbol: SymbolId,
    birth: (ExpressionId, Span),
    owner: CleanupOwnerValueId,
    span: Span,
) -> Result<CleanupConditionId, LoweringError> {
    rooted(owned, symbol, birth.1, Some(birth.0), owner, span, false)
}

/// Generic body paths retain all published incoming edges, including multiple jumps.
pub(super) fn flow(
    owned: &OwnershipCheckedFile,
    symbol: SymbolId,
    origin: Span,
    expression: Option<ExpressionId>,
    owner: CleanupOwnerValueId,
    span: Span,
) -> Result<CleanupConditionId, LoweringError> {
    rooted(owned, symbol, origin, expression, owner, span, true)
}

fn rooted(
    owned: &OwnershipCheckedFile,
    symbol: SymbolId,
    origin: Span,
    expression: Option<ExpressionId>,
    owner: CleanupOwnerValueId,
    span: Span,
    flow: bool,
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
            } if expression.is_none() && *actual == symbol && *at == origin => {
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
            CleanupOwnerValue::Expression {
                expression: actual,
                origin: at,
            } if expression == Some(*actual) && *at == origin => {
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
                    let binding = if flow {
                        let selected = unique(
                            plan.closure_phi_incomings()
                                .iter()
                                .filter(|i| i.kind() == kind),
                        )
                        .ok_or_else(bad)?;
                        if selected.point() != point
                            || selected.boundary() != *boundary
                            || conditions.get(selected.condition()).is_none()
                        {
                            return Err(bad());
                        }
                        let [binding] = selected.bindings() else {
                            return Err(bad());
                        };
                        if binding.target() != phi.owner()
                            || binding.availability_selector() != phi.availability_selector()
                            || binding.presence_source()
                                != IterationPhiPresenceSource::StaticConditions
                            || !binding.capture_slots_to_clear().is_empty()
                            || !binding.root_sources().is_empty()
                            || !binding.selector_writes().is_empty()
                            || !binding.origins().is_empty()
                        {
                            return Err(bad());
                        }
                        binding
                    } else {
                        super::incoming(conditions, plan, phi, kind, point, span)?
                    };
                    count += 1;
                    match binding.values() {
                        [] if flow
                            && conditions.get(binding.available_when())
                                == Some(&CleanupCondition::Never) => {}
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
                let expected = if flow {
                    usize::from(*boundary == IterationPhiBoundary::Header)
                        + plan
                            .exits()
                            .iter()
                            .filter(|exit| match boundary {
                                IterationPhiBoundary::Header => matches!(
                                    exit.kind(),
                                    IterationExitKind::Fallthrough | IterationExitKind::Continue(_)
                                ),
                                IterationPhiBoundary::Exit => matches!(
                                    exit.kind(),
                                    IterationExitKind::Break(_) | IterationExitKind::Exhaustion
                                ),
                            })
                            .count()
                } else if *boundary == IterationPhiBoundary::Header {
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
            CleanupOwnerValue::Parameter { .. } | CleanupOwnerValue::Expression { .. } => {
                return Err(bad());
            }
            _ => return Err(unsupported()),
        }
    }
    if values.iter().any(|(source, condition)| {
        available.get(source).is_none_or(|known| {
            if flow {
                !implies(conditions, *condition, *known)
            } else {
                known != condition
            }
        })
    }) {
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

/// Incoming path conditions may narrow a live phi, but may never create presence.
fn implies(
    conditions: &CleanupConditions,
    path: CleanupConditionId,
    live: CleanupConditionId,
) -> bool {
    fn visit(
        conditions: &CleanupConditions,
        path: CleanupConditionId,
        live: CleanupConditionId,
        seen: &mut BTreeSet<CleanupConditionId>,
    ) -> bool {
        if path == live
            || conditions.get(path) == Some(&CleanupCondition::Never)
            || conditions.get(live) == Some(&CleanupCondition::Always)
        {
            return true;
        }
        if !seen.insert(path) {
            return false;
        }
        let result = match (conditions.get(path), conditions.get(live)) {
            (
                Some(CleanupCondition::Choice { selector, branches }),
                Some(CleanupCondition::Choice {
                    selector: expected,
                    branches: live_branches,
                }),
            ) => {
                if selector == expected {
                    branches.len() == live_branches.len()
                        && branches
                            .iter()
                            .zip(live_branches)
                            .all(|(branch, live)| visit(conditions, *branch, *live, seen))
                } else {
                    branches
                        .iter()
                        .all(|branch| visit(conditions, *branch, live, seen))
                }
            }
            _ => false,
        };
        seen.remove(&path);
        result
    }
    visit(conditions, path, live, &mut BTreeSet::new())
}

/// Compare immutable condition DAGs under this exact exit's saved control path.
/// This reads selector identities; it never evaluates a source condition again.
pub(in super::super) fn equal_on_path(
    conditions: &CleanupConditions,
    actual: CleanupConditionId,
    expected: CleanupConditionId,
    path: CleanupConditionId,
) -> bool {
    use lang_frontend::ownership_checking::CleanupSelectorId;
    enum Value {
        Known(bool),
        Select(CleanupSelectorId, usize),
    }
    fn value(
        conditions: &CleanupConditions,
        id: CleanupConditionId,
        selected: &BTreeMap<CleanupSelectorId, usize>,
    ) -> Option<Value> {
        match conditions.get(id)? {
            CleanupCondition::Always => Some(Value::Known(true)),
            CleanupCondition::Never => Some(Value::Known(false)),
            CleanupCondition::Choice { selector, branches } => match selected.get(selector) {
                Some(index) => value(conditions, *branches.get(*index)?, selected),
                None => Some(Value::Select(*selector, branches.len())),
            },
        }
    }
    fn compare(
        conditions: &CleanupConditions,
        actual: CleanupConditionId,
        expected: CleanupConditionId,
        path: CleanupConditionId,
        selected: &mut BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        if actual == expected {
            return true;
        }
        let Some(path_value) = value(conditions, path, selected) else {
            return false;
        };
        if matches!(path_value, Value::Known(false)) {
            return true;
        }
        let Some(actual_value) = value(conditions, actual, selected) else {
            return false;
        };
        let Some(expected_value) = value(conditions, expected, selected) else {
            return false;
        };
        let pick = match (path_value, actual_value, expected_value) {
            (Value::Known(true), Value::Known(a), Value::Known(b)) => return a == b,
            (Value::Select(selector, count), _, _)
            | (_, Value::Select(selector, count), _)
            | (_, _, Value::Select(selector, count)) => (selector, count),
            _ => return false,
        };
        if pick.1 == 0 {
            return false;
        }
        for branch in 0..pick.1 {
            selected.insert(pick.0, branch);
            if !compare(conditions, actual, expected, path, selected) {
                selected.remove(&pick.0);
                return false;
            }
        }
        selected.remove(&pick.0);
        true
    }
    compare(conditions, actual, expected, path, &mut BTreeMap::new())
}
