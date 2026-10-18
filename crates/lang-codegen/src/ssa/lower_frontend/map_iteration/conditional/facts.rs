//! Exact FE decision chains and source-qualified incoming/Drop obligations.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditionId, CleanupConditions, CleanupOwnerValue, CleanupSelection, CleanupSelectorId,
    CleanupSelectorSource, DropTarget, IterationClosurePhiBinding, IterationPhiBoundary,
    IterationPhiIncoming, IterationPhiIncomingBinding, IterationPhiIncomingKind,
    IterationPhiPresenceSource,
};

pub(in super::super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    symbol: SymbolId,
    shape: Shape,
    span: Span,
) -> Result<Option<source_owner::Proof>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let conditions = owned.cleanup_conditions();
    let descriptor = plan.descriptor();
    if plan.exits().len() != 5
        || plan.closure_phi_incomings().len() != 5
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(unsupported());
    }
    let local_drops: Vec<_> = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == DropTarget::Named(shape.local.symbol))
        .collect();
    if local_drops.len() != 4 {
        return Err(unsupported());
    }
    let local_owner = local_drops
        .first()
        .and_then(|fact| fact.owner())
        .ok_or_else(missing)?;
    if owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.owner() == Some(local_owner))
        .collect::<Vec<_>>()
        != local_drops
    {
        return Err(missing());
    }
    let initializer_span = parsed
        .ast()
        .expressions()
        .get(shape.local.initializer)
        .map_err(|_| missing())?
        .span();
    if !matches!(conditions.owner_value(local_owner), Some(CleanupOwnerValue::Expression { expression, origin })
        if *expression == shape.local.initializer && *origin == initializer_span)
    {
        return Err(missing());
    }
    let proof = if plan.source_access() == MapIterationSourceAccess::Owned {
        Some(source(owned, names, plan, symbol, shape, span)?)
    } else {
        if !plan.closure_phis().is_empty() {
            return Err(unsupported());
        }
        if owned
            .drops()
            .iter()
            .any(|fact| fact.target() == DropTarget::Named(symbol))
        {
            return Err(missing());
        }
        None
    };
    for (index, kind) in shape.kinds().into_iter().enumerate() {
        let exit =
            unique(plan.exits().iter().filter(|exit| exit.kind() == kind)).ok_or_else(missing)?;
        let point = exit_shape::point(kind, descriptor.statement(), shape.body);
        if exit.point() != point
            || owned
                .cleanup_steps()
                .iter()
                .filter_map(|(p, action)| (*p == point).then_some(*action))
                .collect::<Vec<_>>()
                != exit.actions()
        {
            return Err(missing());
        }
        path(conditions, exit.condition(), shape, index, span)?;
        let mut actions = Vec::new();
        if index != 4 {
            actions.push(IterationCleanupAction::Drop(guard_drop(
                owned,
                shape,
                point,
                local_owner,
                span,
            )?));
            actions.extend([
                IterationCleanupAction::EndBinding {
                    statement: descriptor.statement(),
                    symbol: descriptor.value_binding(),
                },
                IterationCleanupAction::EndBinding {
                    statement: descriptor.statement(),
                    symbol: descriptor.key_binding(),
                },
            ]);
        }
        if exit_shape::terminal(kind) {
            actions.extend([
                IterationCleanupAction::FinishProvider(descriptor.statement()),
                IterationCleanupAction::EndSource(descriptor.statement()),
            ]);
            if let Some(proof) = proof {
                actions.push(IterationCleanupAction::Drop(
                    proof.at(point).ok_or_else(missing)?,
                ));
            }
        }
        if exit.actions() != actions {
            return Err(unsupported());
        }
    }
    for (kind, boundary, point, index) in incoming_layout(plan, shape, span)? {
        let incoming = incoming(plan, kind, boundary, point, span)?;
        path(conditions, incoming.condition(), shape, index, span)?;
        if proof.is_none() && !incoming.bindings().is_empty() {
            return Err(missing());
        }
    }
    Ok(proof)
}

/// Only this bounded, ordered decision chain is supported. Unknown or cyclic
/// nodes cannot pass: recursion consumes exactly one of at most three arms.
fn path(
    conditions: &CleanupConditions,
    condition: CleanupConditionId,
    shape: Shape,
    exit: usize,
    span: Span,
) -> Result<(), LoweringError> {
    let choices: &[usize] = match exit {
        0 => &[0],
        1 => &[1, 0],
        2 => &[1, 1, 0],
        3 => &[1, 1, 1],
        4 => &[],
        _ => return Err(error(LoweringErrorKind::MissingFact, span)),
    };
    let mut current = condition;
    for (arm, &selected) in shape.arms.iter().zip(choices) {
        let node = conditions
            .get(current)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let CleanupCondition::Choice { selector, branches } = node else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let source = conditions
            .selector(*selector)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if source.source() != CleanupSelectorSource::Control(arm.control)
            || source.origin() != arm.span
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if source.selection() != CleanupSelection::Branch
            || source.branch_count() != 2
            || branches.len() != 2
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        match conditions.get(branches[1 - selected]) {
            None => return Err(error(LoweringErrorKind::MissingFact, span)),
            Some(CleanupCondition::Never) => {}
            Some(_) => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
        current = branches[selected];
    }
    match conditions.get(current) {
        Some(CleanupCondition::Always) => Ok(()),
        None => Err(error(LoweringErrorKind::MissingFact, span)),
        _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

fn present_path(
    conditions: &CleanupConditions,
    condition: CleanupConditionId,
    selector: CleanupSelectorId,
    shape: Shape,
    exit: usize,
    span: Span,
) -> Result<(), LoweringError> {
    let node = conditions
        .get(condition)
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
    let CleanupCondition::Choice {
        selector: actual,
        branches,
    } = node
    else {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    };
    if *actual != selector {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    let [absent, present] = branches.as_slice() else {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    };
    match conditions.get(*absent) {
        None => return Err(error(LoweringErrorKind::MissingFact, span)),
        Some(CleanupCondition::Never) => {}
        Some(_) => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
    }
    path(conditions, *present, shape, exit, span)
}

fn incoming_layout(
    plan: &MapIterationOwnershipPlan,
    shape: Shape,
    span: Span,
) -> Result<
    [(
        IterationPhiIncomingKind,
        IterationPhiBoundary,
        DropPoint,
        usize,
    ); 5],
    LoweringError,
> {
    let IterationExitKind::Continue(continued) = shape.arms[0].kind else {
        return Err(error(LoweringErrorKind::MissingFact, span));
    };
    let IterationExitKind::Break(broken) = shape.arms[1].kind else {
        return Err(error(LoweringErrorKind::MissingFact, span));
    };
    let statement = plan.descriptor().statement();
    Ok([
        (
            IterationPhiIncomingKind::Entry,
            IterationPhiBoundary::Header,
            DropPoint::AfterExpression(plan.descriptor().source()),
            4,
        ),
        (
            IterationPhiIncomingKind::Continue(continued),
            IterationPhiBoundary::Header,
            DropPoint::ControlTransfer(continued),
            0,
        ),
        (
            IterationPhiIncomingKind::Break(broken),
            IterationPhiBoundary::Exit,
            DropPoint::ControlTransfer(broken),
            1,
        ),
        (
            IterationPhiIncomingKind::Fallthrough,
            IterationPhiBoundary::Header,
            DropPoint::AfterStatement(shape.body),
            3,
        ),
        (
            IterationPhiIncomingKind::Exhaustion,
            IterationPhiBoundary::Exit,
            DropPoint::LoopExit(statement),
            4,
        ),
    ])
}
fn incoming(
    plan: &MapIterationOwnershipPlan,
    kind: IterationPhiIncomingKind,
    boundary: IterationPhiBoundary,
    point: DropPoint,
    span: Span,
) -> Result<&IterationPhiIncoming, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let incoming = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|incoming| incoming.kind() == kind),
    )
    .ok_or_else(missing)?;
    if incoming.boundary() != boundary || incoming.point() != point {
        return Err(missing());
    }
    Ok(incoming)
}
fn binding<'a>(
    incoming: &'a IterationPhiIncoming,
    phi: &IterationClosurePhiBinding,
    span: Span,
) -> Result<&'a IterationPhiIncomingBinding, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let [binding] = incoming.bindings() else {
        return Err(missing());
    };
    if binding.target() != phi.owner()
        || binding.availability_selector() != phi.availability_selector()
        || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
        || !binding.capture_slots_to_clear().is_empty()
        || !binding.root_sources().is_empty()
        || !binding.selector_writes().is_empty()
        || !binding.origins().is_empty()
    {
        return Err(missing());
    }
    Ok(binding)
}
fn source(
    owned: &OwnershipCheckedFile,
    names: &NameResolution,
    plan: &MapIterationOwnershipPlan,
    symbol: SymbolId,
    shape: Shape,
    span: Span,
) -> Result<source_owner::Proof, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let conditions = owned.cleanup_conditions();
    let origin = names
        .symbols()
        .get(symbol.index())
        .ok_or_else(missing)?
        .span();
    // Screen the supported owner lifetime before demanding this phi topology.
    // A live source after the loop is valid FE output outside this slice.
    let drops: Vec<_> = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == DropTarget::Named(symbol))
        .collect();
    if drops.iter().any(|fact| fact.value_origin() != origin) {
        return Err(missing());
    }
    let points = [4, 1, 2].map(|index| {
        exit_shape::point(
            shape.kinds()[index],
            plan.descriptor().statement(),
            shape.body,
        )
    });
    if drops.len() != 3 || drops.iter().any(|fact| !points.contains(&fact.point())) {
        return Err(unsupported());
    }
    if plan.closure_phis().len() != 2 {
        return Err(unsupported());
    }
    let header = source_owner::phi(conditions, plan, symbol, IterationPhiBoundary::Header, span)?;
    let exit = source_owner::phi(conditions, plan, symbol, IterationPhiBoundary::Exit, span)?;
    if header.owner() == exit.owner()
        || header.availability_selector() == exit.availability_selector()
    {
        return Err(missing());
    }
    let mut entry_owner = None;
    for (kind, boundary, point, index) in incoming_layout(plan, shape, span)? {
        let incoming = incoming(plan, kind, boundary, point, span)?;
        path(conditions, incoming.condition(), shape, index, span)?;
        let binding = binding(
            incoming,
            if boundary == IterationPhiBoundary::Header {
                header
            } else {
                exit
            },
            span,
        )?;
        if boundary == IterationPhiBoundary::Exit {
            match conditions.get(binding.available_when()) {
                None => return Err(missing()),
                Some(CleanupCondition::Never) if binding.values().is_empty() => {}
                Some(_) => return Err(unsupported()),
            }
        } else {
            let [value] = binding.values() else {
                return Err(missing());
            };
            if value.condition() != binding.available_when() {
                return Err(missing());
            }
            if kind == IterationPhiIncomingKind::Entry {
                match conditions.get(binding.available_when()) {
                    None => return Err(missing()),
                    Some(CleanupCondition::Always) => {}
                    Some(_) => return Err(unsupported()),
                }
                match conditions.owner_value(value.source()) {
                    Some(CleanupOwnerValue::Parameter {
                        symbol: actual,
                        origin: actual_origin,
                    }) if *actual == symbol && *actual_origin == origin => {}
                    None | Some(CleanupOwnerValue::Parameter { .. }) => return Err(missing()),
                    Some(_) => return Err(unsupported()),
                }
                entry_owner = Some(value.source());
            } else {
                if value.source() != header.owner() {
                    return Err(missing());
                }
                present_path(
                    conditions,
                    value.condition(),
                    header.availability_selector(),
                    shape,
                    index,
                    span,
                )?;
            }
        }
    }
    let entry_owner = entry_owner.ok_or_else(missing)?;
    let related: Vec<_> = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| {
            fact.target() == DropTarget::Named(symbol)
                || fact.owner().is_some_and(|owner| {
                    [entry_owner, header.owner(), exit.owner()].contains(&owner)
                })
        })
        .collect();
    if related != drops {
        return Err(missing());
    }
    let mut selected = Vec::new();
    for index in [4, 1, 2] {
        let kind = shape.kinds()[index];
        let point = exit_shape::point(kind, plan.descriptor().statement(), shape.body);
        let fact = unique(drops.iter().copied().filter(|fact| fact.point() == point))
            .ok_or_else(missing)?;
        if fact.owner() != Some(header.owner())
            || fact.value_origin() != origin
            || fact.capture_slot().is_some()
            || fact.instance_address().is_some()
        {
            return Err(missing());
        }
        present_path(
            conditions,
            fact.condition().ok_or_else(missing)?,
            header.availability_selector(),
            shape,
            index,
            span,
        )?;
        selected.push(fact);
    }
    Ok(source_owner::Proof {
        drop: selected[0],
        transfer: Some(selected[1]),
        conditional_return: Some(selected[2]),
    })
}
