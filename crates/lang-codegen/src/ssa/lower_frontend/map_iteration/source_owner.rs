//! Exact source-owner proof for the observed normal and direct-tail Map shapes.
//! A source facts were first collected by the ordinary 12-path run on 9244391b.
//! No shape is inferred from Sequential loops or accepted from a presence bit alone.
mod future;
pub(super) mod provenance;
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditions, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource, DropFact,
    DropTarget, IterationClosurePhiBinding, IterationPhiBoundary, IterationPhiIncomingBinding,
    IterationPhiIncomingKind, IterationPhiPresenceSource,
};

#[derive(Clone, Copy)]
pub(super) struct Proof {
    pub(super) flow: Option<Flow>,
    pub(super) drop: DropFact,
    pub(super) transfer: Option<DropFact>,
    pub(super) conditional_return: Option<DropFact>,
}

/// A small capability handle into the same immutable ownership product.
#[derive(Clone, Copy)]
pub(super) struct Flow {
    pub(super) statement: StatementId,
    pub(super) source: SourceIdentity,
    pub(super) span: Span,
    /// The unique same-epoch lexical Drop proved outside this provider's exits.
    pub(super) late: Option<DropFact>,
}

impl Proof {
    pub(super) fn at_checked(
        self,
        point: DropPoint,
        owned: &OwnershipCheckedFile,
    ) -> Result<Option<DropFact>, LoweringError> {
        match self.flow {
            Some(flow) => {
                let plan = owned.map_iteration(flow.statement).ok_or_else(|| {
                    error(LoweringErrorKind::MissingFact, self.drop.value_origin())
                })?;
                if plan.exits().iter().any(|exit| exit.point() == point) {
                    conditional::source_drop(owned, flow, point)
                } else {
                    flow.late
                        .filter(|fact| fact.point() == point && owned.drops().contains(fact))
                        .map(Some)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, flow.span))
                }
            }
            None => Ok(self.at(point)),
        }
    }

    pub(super) fn at(self, point: DropPoint) -> Option<DropFact> {
        if self.drop.point() == point {
            Some(self.drop)
        } else {
            self.transfer
                .into_iter()
                .chain(self.conditional_return)
                .find(|fact| fact.point() == point)
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
    qualify_with_birth(parsed, names, owned, plan, (symbol, None), body, body_exit)
}

pub(super) fn qualify_formed(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    root: (SymbolId, &crate::ssa::map_source::Formation<SymbolId>),
    body: StatementId,
    body_exit: IterationExitKind,
) -> Result<Proof, LoweringError> {
    qualify_with_birth(
        parsed,
        names,
        owned,
        plan,
        (root.0, Some(root.1)),
        body,
        body_exit,
    )
}

fn qualify_with_birth(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    root: (
        SymbolId,
        Option<&crate::ssa::map_source::Formation<SymbolId>>,
    ),
    body: StatementId,
    body_exit: IterationExitKind,
) -> Result<Proof, LoweringError> {
    let (symbol, birth) = root;
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
    let source_drops = if let Some(formation) = birth {
        formed::drops(parsed, owned, symbol, formation)?
    } else {
        owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Named(symbol))
            .collect::<Vec<_>>()
    };
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
        return future::qualify(parsed, names, owned, plan, root, body_exit, span);
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
    let available = if let Some(birth) = birth {
        formed_condition(parsed, owned, (symbol, birth.producer), seed.source(), span)?
    } else {
        provenance::parameter(owned, symbol, origin, seed.source(), span)?
    };
    if seed.condition() != entry.available_when() || seed.condition() != available {
        return Err(missing());
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
            source_drops.contains(fact)
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
    Ok(Proof {
        flow: None,
        drop,
        transfer,
        conditional_return: None,
    })
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

pub(super) fn phi<'a>(
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

pub(super) fn incoming<'a>(
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
    typed.map_iterations().iter().any(|descriptor| {
        qualify::qualify(parsed, names, typed, owned, *descriptor)
            .ok()
            .and_then(|q| q.owner)
            .is_some_and(|proof| {
                proof
                    .at_checked(fact.point(), owned)
                    .is_ok_and(|actual| actual == Some(fact))
            })
    })
}

/// Resolve the complete live FE graph to the real local producer, including prior providers.
pub(super) fn formed_condition(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    root: (SymbolId, ExpressionId),
    owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
    span: Span,
) -> Result<lang_frontend::ownership_checking::CleanupConditionId, LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(root.1)
        .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
    provenance::formed(owned, root.0, (root.1, node.span()), owner, span)
}

/// Validate the complete published owner graph against this precise source birth.
pub(super) fn flow_condition(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    flow: Flow,
    owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
    span: Span,
) -> Result<lang_frontend::ownership_checking::CleanupConditionId, LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let plan = owned.map_iteration(flow.statement).ok_or_else(bad)?;
    match flow.source {
        SourceIdentity::Parameter(symbol) => {
            let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
            provenance::flow(owned, symbol, origin, None, owner, span)
        }
        SourceIdentity::Local {
            symbol,
            initializer: producer,
        }
        | SourceIdentity::Formed { symbol, producer } => {
            let node = parsed
                .ast()
                .expressions()
                .get(producer)
                .map_err(|_| bad())?;
            provenance::flow(owned, symbol, node.span(), Some(producer), owner, span)
        }
        SourceIdentity::Temporary(expression) => {
            let origin = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| bad())?
                .span();
            if owned.cleanup_conditions().owner_value(owner)
                != Some(&CleanupOwnerValue::Expression { expression, origin })
            {
                return Err(bad());
            }
            let entry = unique(
                plan.closure_phi_incomings()
                    .iter()
                    .filter(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry),
            )
            .ok_or_else(bad)?;
            if owned.cleanup_conditions().get(entry.condition()) != Some(&CleanupCondition::Always)
            {
                return Err(bad());
            }
            Ok(entry.condition())
        }
    }
}
