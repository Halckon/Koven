//! Exact source-owner proof for the observed normal and direct-tail Map shapes.
//! A source facts were first collected by the ordinary 12-path run on 9244391b.
//! No shape is inferred from Sequential loops or accepted from a presence bit alone.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditions, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource, DropFact,
    DropTarget, IterationClosurePhiBinding, IterationPhiBoundary, IterationPhiIncomingBinding,
    IterationPhiIncomingKind, IterationPhiPresenceSource,
};

#[derive(Clone, Copy)]
pub(super) struct Proof {
    pub(super) drop: DropFact,
    transfer: Option<DropFact>,
}

impl Proof {
    pub(super) fn at(self, point: DropPoint) -> Option<DropFact> {
        if self.drop.point() == point {
            Some(self.drop)
        } else {
            self.transfer.filter(|fact| fact.point() == point)
        }
    }
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    symbol: SymbolId,
    body: StatementId,
    body_exit: IterationExitKind,
) -> Result<Proof, LoweringError> {
    let descriptor = plan.descriptor();
    let node = parsed
        .ast()
        .statements()
        .get(descriptor.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let origin = names
        .symbols()
        .get(symbol.index())
        .ok_or_else(missing)?
        .span();
    let transfer_point = match body_exit {
        IterationExitKind::Break(expression) | IterationExitKind::Return(expression) => {
            Some(DropPoint::ControlTransfer(expression))
        }
        IterationExitKind::Fallthrough | IterationExitKind::Continue(_) => None,
        IterationExitKind::Exhaustion => return Err(missing()),
    };
    let exhaustion_point = DropPoint::LoopExit(descriptor.statement());
    let source_drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == DropTarget::Named(symbol))
        .collect::<Vec<_>>();
    if source_drops
        .iter()
        .any(|fact| fact.value_origin() != origin)
    {
        return Err(missing());
    }
    // A later source lifetime is legal frontend output, but is not this proof.
    if source_drops.len() != 1 + usize::from(transfer_point.is_some())
        || source_drops
            .iter()
            .any(|fact| fact.point() != exhaustion_point && Some(fact.point()) != transfer_point)
    {
        return Err(unsupported());
    }
    let expected_incomings = if matches!(body_exit, IterationExitKind::Return(_)) {
        2
    } else {
        3
    };
    if plan.closure_phis().len() != 2
        || plan.closure_phi_incomings().len() != expected_incomings
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(unsupported());
    }
    let conditions = owned.cleanup_conditions();
    let header = phi(conditions, plan, symbol, IterationPhiBoundary::Header, span)?;
    let exit = phi(conditions, plan, symbol, IterationPhiBoundary::Exit, span)?;
    if header.owner() == exit.owner()
        || header.availability_selector() == exit.availability_selector()
        || header.availability_condition() == exit.availability_condition()
    {
        return Err(missing());
    }
    let entry = incoming(
        conditions,
        plan,
        header,
        IterationPhiIncomingKind::Entry,
        DropPoint::AfterExpression(descriptor.source()),
        span,
    )?;
    let [seed] = entry.values() else {
        return Err(missing());
    };
    if conditions.get(entry.available_when()) != Some(&CleanupCondition::Always)
        || seed.condition() != entry.available_when()
    {
        return Err(missing());
    }
    match conditions.owner_value(seed.source()) {
        Some(CleanupOwnerValue::Parameter {
            symbol: actual,
            origin: actual_origin,
        }) if *actual == symbol && *actual_origin == origin => {}
        Some(CleanupOwnerValue::Parameter { .. }) | None => return Err(missing()),
        Some(_) => return Err(unsupported()),
    }
    match body_exit {
        IterationExitKind::Fallthrough | IterationExitKind::Continue(_) => {
            let kind = match body_exit {
                IterationExitKind::Continue(expression) => {
                    IterationPhiIncomingKind::Continue(expression)
                }
                _ => IterationPhiIncomingKind::Fallthrough,
            };
            let backedge = incoming(
                conditions,
                plan,
                header,
                kind,
                exit_shape::point(body_exit, descriptor.statement(), body),
                span,
            )?;
            let [value] = backedge.values() else {
                return Err(missing());
            };
            if value.source() != header.owner()
                || value.condition() != header.availability_condition()
                || backedge.available_when() != header.availability_condition()
            {
                return Err(missing());
            }
        }
        IterationExitKind::Break(expression) => {
            let broken = incoming(
                conditions,
                plan,
                exit,
                IterationPhiIncomingKind::Break(expression),
                DropPoint::ControlTransfer(expression),
                span,
            )?;
            absent(conditions, broken, span)?;
        }
        // Return has no phi incoming. The real result and terminator are verified
        // by the Map carrier after the complete ControlTransfer actions execute.
        IterationExitKind::Return(_) => {}
        IterationExitKind::Exhaustion => return Err(missing()),
    }
    let exhausted = incoming(
        conditions,
        plan,
        exit,
        IterationPhiIncomingKind::Exhaustion,
        exhaustion_point,
        span,
    )?;
    absent(conditions, exhausted, span)?;
    let related = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| {
            fact.target() == DropTarget::Named(symbol)
                || fact.owner().is_some_and(|owner| {
                    [seed.source(), header.owner(), exit.owner()].contains(&owner)
                })
        })
        .collect::<Vec<_>>();
    if related != source_drops {
        return Err(missing());
    }
    for fact in &source_drops {
        if fact.owner() != Some(header.owner())
            || fact.condition() != Some(header.availability_condition())
            || fact.capture_slot().is_some()
            || fact.instance_address().is_some()
        {
            return Err(missing());
        }
    }
    let drop = unique(
        source_drops
            .iter()
            .copied()
            .filter(|fact| fact.point() == exhaustion_point),
    )
    .ok_or_else(missing)?;
    let transfer = transfer_point
        .map(|point| {
            unique(
                source_drops
                    .iter()
                    .copied()
                    .filter(|fact| fact.point() == point),
            )
            .ok_or_else(missing)
        })
        .transpose()?;
    for (kind, fact) in [
        (body_exit, transfer),
        (IterationExitKind::Exhaustion, Some(drop)),
    ] {
        let exit =
            unique(plan.exits().iter().filter(|exit| exit.kind() == kind)).ok_or_else(missing)?;
        let actual = exit
            .actions()
            .iter()
            .filter_map(|action| match action {
                IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::Named(symbol) =>
                {
                    Some(*fact)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if actual != fact.into_iter().collect::<Vec<_>>() {
            return Err(missing());
        }
        if let Some(fact) = fact {
            let terminal = [
                IterationCleanupAction::FinishProvider(descriptor.statement()),
                IterationCleanupAction::EndSource(descriptor.statement()),
                IterationCleanupAction::Drop(fact),
            ];
            if !exit.actions().ends_with(&terminal)
                || (kind == IterationExitKind::Exhaustion && exit.actions() != terminal)
            {
                return Err(unsupported());
            }
        }
    }
    Ok(Proof { drop, transfer })
}

fn absent(
    conditions: &CleanupConditions,
    incoming: &IterationPhiIncomingBinding,
    span: Span,
) -> Result<(), LoweringError> {
    match conditions.get(incoming.available_when()) {
        None => Err(error(LoweringErrorKind::MissingFact, span)),
        Some(CleanupCondition::Never) if incoming.values().is_empty() => Ok(()),
        _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

fn phi<'a>(
    conditions: &CleanupConditions,
    plan: &'a MapIterationOwnershipPlan,
    symbol: SymbolId,
    boundary: IterationPhiBoundary,
    span: Span,
) -> Result<&'a IterationClosurePhiBinding, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let statement = plan.descriptor().statement();
    let phi = unique(
        plan.closure_phis()
            .iter()
            .filter(|phi| phi.symbol() == symbol && phi.boundary() == boundary),
    )
    .ok_or_else(missing)?;
    if !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
        statement: actual, boundary: actual_boundary, symbol: actual_symbol, origin,
    }) if *actual == statement && *actual_boundary == boundary && *actual_symbol == symbol && *origin == span)
        || !phi.root_nodes().is_empty()
        || !phi.capture_layout().is_empty()
        || !phi.origins().is_empty()
    {
        return Err(missing());
    }
    let selector = conditions
        .selector(phi.availability_selector())
        .ok_or_else(missing)?;
    if !matches!(selector.source(), CleanupSelectorSource::IterationPhi { statement: actual, boundary: actual_boundary, .. }
        if actual == statement && actual_boundary == boundary)
        || selector.selection() != CleanupSelection::IterationPresence
        || selector.origin() != span
        || selector.branch_count() != 2
    {
        return Err(missing());
    }
    let Some(CleanupCondition::Choice { selector, branches }) =
        conditions.get(phi.availability_condition())
    else {
        return Err(missing());
    };
    let [absent, present] = branches.as_slice() else {
        return Err(missing());
    };
    if *selector != phi.availability_selector()
        || conditions.get(*absent) != Some(&CleanupCondition::Never)
        || conditions.get(*present) != Some(&CleanupCondition::Always)
    {
        return Err(missing());
    }
    Ok(phi)
}

fn incoming<'a>(
    conditions: &CleanupConditions,
    plan: &'a MapIterationOwnershipPlan,
    phi: &IterationClosurePhiBinding,
    kind: IterationPhiIncomingKind,
    point: DropPoint,
    span: Span,
) -> Result<&'a IterationPhiIncomingBinding, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let incoming = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|incoming| incoming.kind() == kind),
    )
    .ok_or_else(missing)?;
    if incoming.point() != point
        || incoming.boundary() != phi.boundary()
        || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
    {
        return Err(missing());
    }
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

pub(in super::super) fn supported_drop(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> bool {
    let descriptor = match fact.point() {
        DropPoint::LoopExit(statement) => typed.map_iteration(statement),
        DropPoint::ControlTransfer(expression) => unique(typed.map_iterations().iter().filter(|descriptor| {
            owned.map_iteration(descriptor.statement()).is_some_and(|plan| {
                plan.exits().iter().any(|exit| {
                    matches!(exit.kind(), IterationExitKind::Break(actual) | IterationExitKind::Return(actual) if actual == expression)
                        && exit.point() == fact.point()
                        && exit.actions().contains(&IterationCleanupAction::Drop(fact))
                })
            })
        })),
        _ => None,
    };
    let Some(descriptor) = descriptor else {
        return false;
    };
    qualify::qualify(parsed, names, typed, owned, *descriptor)
        .ok()
        .and_then(|qualified| qualified.owner)
        .is_some_and(|proof| proof.at(fact.point()) == Some(fact))
}
