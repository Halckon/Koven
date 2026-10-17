//! Narrow normal-exhaustion proof for the Map source itself.
//! The shape was observed on the SPEC0331 ordinary owned String source, not
//! inferred from an unrelated Sequential provider or a presence-bit name.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditions, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource, DropFact,
    DropTarget, IterationClosurePhiBinding, IterationPhiBoundary, IterationPhiIncomingBinding,
    IterationPhiIncomingKind, IterationPhiPresenceSource,
};

#[derive(Clone, Copy)]
pub(super) struct Proof {
    pub(super) drop: DropFact,
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    symbol: SymbolId,
    body: StatementId,
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
    let origin = names
        .symbols()
        .get(symbol.index())
        .ok_or_else(missing)?
        .span();
    // A live source after this loop is valid FE output, but outside this
    // exhaustion-only source-owner proof. Check lifetime shape before phis.
    let source_drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(symbol))
        .collect::<Vec<_>>();
    if source_drops
        .iter()
        .any(|fact| fact.value_origin() != origin)
    {
        return Err(missing());
    }
    if !matches!(source_drops.as_slice(), [fact]
        if fact.point() == DropPoint::LoopExit(descriptor.statement()))
    {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    }
    if plan.closure_phis().len() != 2
        || plan.closure_phi_incomings().len() != 3
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
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
        Some(_) => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
    }
    let backedge = incoming(
        conditions,
        plan,
        header,
        IterationPhiIncomingKind::Fallthrough,
        DropPoint::AfterStatement(body),
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
    let exhausted = incoming(
        conditions,
        plan,
        exit,
        IterationPhiIncomingKind::Exhaustion,
        DropPoint::LoopExit(descriptor.statement()),
        span,
    )?;
    if conditions.get(exhausted.available_when()).is_none() {
        return Err(missing());
    }
    if conditions.get(exhausted.available_when()) != Some(&CleanupCondition::Never)
        || !exhausted.values().is_empty()
    {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    }
    let fact = *unique(owned.drops().iter().filter(|fact| {
        fact.target() == DropTarget::Named(symbol)
            || fact
                .owner()
                .is_some_and(|owner| [seed.source(), header.owner(), exit.owner()].contains(&owner))
    }))
    .ok_or_else(missing)?;
    if fact.target() != DropTarget::Named(symbol)
        || fact.point() != DropPoint::LoopExit(descriptor.statement())
        || fact.value_origin() != origin
        || fact.owner() != Some(header.owner())
        || fact.condition() != Some(header.availability_condition())
        || fact.capture_slot().is_some()
        || fact.instance_address().is_some()
    {
        return Err(missing());
    }
    let exhaustion = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
    )
    .ok_or_else(missing)?;
    if exhaustion.actions()
        != [
            IterationCleanupAction::FinishProvider(descriptor.statement()),
            IterationCleanupAction::EndSource(descriptor.statement()),
            IterationCleanupAction::Drop(fact),
        ]
    {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    }
    Ok(Proof { drop: fact })
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
    let DropPoint::LoopExit(statement) = fact.point() else {
        return false;
    };
    let Some(descriptor) = typed.map_iteration(statement) else {
        return false;
    };
    qualify::qualify(parsed, names, typed, owned, *descriptor)
        .ok()
        .and_then(|qualified| qualified.owner)
        .is_some_and(|proof| proof.drop == fact)
}
