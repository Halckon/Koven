//! Complete two-/four-column normal local proof, including absent old columns.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditions, CleanupOwnerValue, CleanupSelectorSource, IterationClosurePhiBinding,
    IterationPhiBoundary, IterationPhiIncoming, IterationPhiIncomingBinding,
    IterationPhiIncomingKind, IterationPhiPresenceSource,
};

pub(super) fn prove(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    site: &identity::Site<'_>,
    previous: Option<(&identity::Site<'_>, &Proof)>,
) -> Result<Proof, LoweringError> {
    let span = site.span;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let plan = site.plan;
    let descriptor = plan.descriptor();
    let conditions = owned.cleanup_conditions();
    if !plan.capture_graph().nodes().is_empty() {
        return Err(unsupported());
    }
    let mut roots = previous
        .map(|(site, _)| vec![site.root])
        .unwrap_or_default();
    roots.push(site.root);
    roots.sort();
    if plan
        .closure_phis()
        .iter()
        .any(|phi| !roots.contains(&phi.symbol()))
    {
        return Err(unsupported());
    }
    if plan.closure_phis().len() != roots.len() * 2 || plan.closure_phi_incomings().len() != 3 {
        return Err(missing());
    }
    let headers = phis(conditions, plan, &roots, IterationPhiBoundary::Header, span)?;
    let exits = phis(conditions, plan, &roots, IterationPhiBoundary::Exit, span)?;
    let all = headers.iter().chain(&exits).copied().collect::<Vec<_>>();
    if all.iter().enumerate().any(|(index, phi)| {
        all[..index].iter().any(|prior| {
            prior.owner() == phi.owner()
                || prior.availability_selector() == phi.availability_selector()
                || prior.availability_condition() == phi.availability_condition()
        })
    }) {
        return Err(missing());
    }
    let entry = incoming(
        plan,
        IterationPhiIncomingKind::Entry,
        IterationPhiBoundary::Header,
        DropPoint::AfterExpression(descriptor.source()),
        &headers,
        span,
    )?;
    let back = incoming(
        plan,
        IterationPhiIncomingKind::Fallthrough,
        IterationPhiBoundary::Header,
        DropPoint::AfterStatement(site.body),
        &headers,
        span,
    )?;
    let exhausted = incoming(
        plan,
        IterationPhiIncomingKind::Exhaustion,
        IterationPhiBoundary::Exit,
        DropPoint::LoopExit(descriptor.statement()),
        &exits,
        span,
    )?;
    let path = conditions::path(parsed, conditions, entry.condition(), previous, span)?;
    for incoming in [entry, back, exhausted] {
        conditions::check(
            conditions,
            incoming.condition(),
            path,
            None,
            conditions::Expected::Path,
            span,
        )?;
    }
    let current = roots
        .iter()
        .position(|root| *root == site.root)
        .ok_or_else(missing)?;
    let header = headers[current];
    let exit = exits[current];
    let entry_binding = &entry.bindings()[current];
    let [seed] = entry_binding.values() else {
        return Err(missing());
    };
    if seed.condition() != entry_binding.available_when()
        || all.iter().any(|phi| phi.owner() == seed.source())
        || previous.is_some_and(|(_, proof)| seed.source() == proof.birth)
    {
        return Err(missing());
    }
    conditions::check(
        conditions,
        seed.condition(),
        path,
        None,
        conditions::Expected::Path,
        span,
    )?;
    let initializer = parsed
        .ast()
        .expressions()
        .get(site.initializer)
        .map_err(|_| missing())?;
    if !matches!(conditions.owner_value(seed.source()), Some(CleanupOwnerValue::Expression { expression, origin })
        if *expression == site.initializer && *origin == initializer.span())
    {
        // Once the ordinary local Call is admitted, Parameter/other owner kinds
        // are corrupt anchors, rather than alternate local production paths.
        return Err(missing());
    }
    self_edge(conditions, &back.bindings()[current], header, path, span)?;
    let late_size = site.guard.map(|guard| guard.size);
    let drop_point = late_size.map_or(
        DropPoint::LoopExit(descriptor.statement()),
        DropPoint::CallReturn,
    );
    let drop_owner = if late_size.is_some() { exit } else { header };
    let drop = drop_fact(
        owned,
        site.root,
        site.origin,
        drop_point,
        drop_owner,
        path,
        span,
    )?;
    if late_size.is_some() {
        let binding = &exhausted.bindings()[current];
        let [value] = binding.values() else {
            return Err(missing());
        };
        if value.source() != header.owner() || value.condition() != binding.available_when() {
            return Err(missing());
        }
        // The whole exhaustion incoming is guarded by P. FE publishes the
        // transferred H presence itself here, distinct from P-and-H on backedge.
        conditions::check(
            conditions,
            binding.available_when(),
            path,
            Some(header.availability_selector()),
            conditions::Expected::Presence,
            span,
        )?;
    } else {
        absent(conditions, &exhausted.bindings()[current], span)?;
    }
    let mut dead = Vec::new();
    if let Some((prior, proof)) = previous {
        let index = roots
            .iter()
            .position(|root| *root == prior.root)
            .ok_or_else(missing)?;
        let header = headers[index];
        let exit = exits[index];
        // Absence is anchored at Entry; the sole backedge can preserve only this
        // same absent H. There is no incoming source from which an owner arises.
        absent(conditions, &entry.bindings()[index], span)?;
        self_edge(conditions, &back.bindings()[index], header, path, span)?;
        absent(conditions, &exhausted.bindings()[index], span)?;
        let dead_drop = drop_fact(
            owned,
            prior.root,
            prior.origin,
            DropPoint::LoopExit(descriptor.statement()),
            header,
            path,
            span,
        )?;
        dead.push(DeadColumn {
            root: prior.root,
            initializer: prior.initializer,
            previous: prior.plan.descriptor().statement(),
            consumed: proof.drop,
            drop: dead_drop,
            header: header.owner(),
            exit: exit.owner(),
        });
    }
    cleanup(
        owned,
        site,
        path,
        late_size.is_none().then_some(drop),
        &dead,
    )?;
    Ok(Proof {
        root: site.root,
        initializer: site.initializer,
        consumer: site.consumer,
        birth: seed.source(),
        header: header.owner(),
        exit: exit.owner(),
        drop,
        late_size,
        dead,
        path,
    })
}

fn phis<'a>(
    conditions: &CleanupConditions,
    plan: &'a MapIterationOwnershipPlan,
    roots: &[SymbolId],
    boundary: IterationPhiBoundary,
    span: Span,
) -> Result<Vec<&'a IterationClosurePhiBinding>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let flow = match boundary {
        IterationPhiBoundary::Header => plan.closure_flow().header(),
        IterationPhiBoundary::Exit => plan.closure_flow().exit(),
    };
    if flow.len() != roots.len()
        || flow
            .iter()
            .zip(roots)
            .any(|(binding, root)| binding.symbol() != *root || !binding.origins().is_empty())
    {
        return Err(missing());
    }
    let result = roots
        .iter()
        .map(|root| source_owner::phi(conditions, plan, *root, boundary, span))
        .collect::<Result<Vec<_>, _>>()?;
    let actual = plan
        .closure_phis()
        .iter()
        .filter(|phi| phi.boundary() == boundary)
        .collect::<Vec<_>>();
    if actual != result {
        return Err(missing());
    }
    for (slot, phi) in result.iter().enumerate() {
        let selector = conditions
            .selector(phi.availability_selector())
            .ok_or_else(missing)?;
        if selector.source()
            != (CleanupSelectorSource::IterationPhi {
                statement: plan.descriptor().statement(),
                boundary,
                slot,
            })
        {
            return Err(missing());
        }
    }
    Ok(result)
}

fn incoming<'a>(
    plan: &'a MapIterationOwnershipPlan,
    kind: IterationPhiIncomingKind,
    boundary: IterationPhiBoundary,
    point: DropPoint,
    phis: &[&IterationClosurePhiBinding],
    span: Span,
) -> Result<&'a IterationPhiIncoming, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let incoming = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|incoming| incoming.kind() == kind),
    )
    .ok_or_else(missing)?;
    if incoming.boundary() != boundary
        || incoming.point() != point
        || incoming.bindings().len() != phis.len()
    {
        return Err(missing());
    }
    for (binding, phi) in incoming.bindings().iter().zip(phis) {
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
    }
    Ok(incoming)
}

fn absent(
    conditions: &CleanupConditions,
    binding: &IterationPhiIncomingBinding,
    span: Span,
) -> Result<(), LoweringError> {
    if conditions.get(binding.available_when()) != Some(&CleanupCondition::Never)
        || !binding.values().is_empty()
    {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    Ok(())
}

fn self_edge(
    conditions: &CleanupConditions,
    binding: &IterationPhiIncomingBinding,
    header: &IterationClosurePhiBinding,
    path: Option<Path>,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let [value] = binding.values() else {
        return Err(missing());
    };
    if value.source() != header.owner() || value.condition() != binding.available_when() {
        return Err(missing());
    }
    conditions::check(
        conditions,
        binding.available_when(),
        path,
        Some(header.availability_selector()),
        conditions::Expected::Conjunction,
        span,
    )
}

fn drop_fact(
    owned: &OwnershipCheckedFile,
    root: SymbolId,
    origin: Span,
    point: DropPoint,
    phi: &IterationClosurePhiBinding,
    path: Option<Path>,
    span: Span,
) -> Result<DropFact, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let fact = unique(
        owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Named(root) && fact.point() == point),
    )
    .ok_or_else(missing)?;
    if fact.owner() != Some(phi.owner())
        || fact.value_origin() != origin
        || fact.capture_slot().is_some()
        || fact.instance_address().is_some()
    {
        return Err(missing());
    }
    conditions::check(
        owned.cleanup_conditions(),
        fact.condition().ok_or_else(missing)?,
        path,
        Some(phi.availability_selector()),
        conditions::Expected::Conjunction,
        span,
    )?;
    Ok(fact)
}

fn cleanup(
    owned: &OwnershipCheckedFile,
    site: &identity::Site<'_>,
    path: Option<Path>,
    current_drop: Option<DropFact>,
    dead: &[DeadColumn],
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, site.span);
    let plan = site.plan;
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    if plan.exits().len() != 2 {
        return Err(missing());
    }
    for kind in [
        IterationExitKind::Fallthrough,
        IterationExitKind::Exhaustion,
    ] {
        let exit =
            unique(plan.exits().iter().filter(|exit| exit.kind() == kind)).ok_or_else(missing)?;
        let point = exit_shape::point(kind, statement, site.body);
        conditions::check(
            owned.cleanup_conditions(),
            exit.condition(),
            path,
            None,
            conditions::Expected::Path,
            site.span,
        )?;
        let expected = if kind == IterationExitKind::Fallthrough {
            vec![
                IterationCleanupAction::EndBinding {
                    statement,
                    symbol: descriptor.value_binding(),
                },
                IterationCleanupAction::EndBinding {
                    statement,
                    symbol: descriptor.key_binding(),
                },
            ]
        } else {
            [
                IterationCleanupAction::FinishProvider(statement),
                IterationCleanupAction::EndSource(statement),
            ]
            .into_iter()
            .chain(current_drop.into_iter().map(IterationCleanupAction::Drop))
            .chain(
                dead.iter()
                    .map(|dead| IterationCleanupAction::Drop(dead.drop)),
            )
            .collect()
        };
        let actual = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(actual, action)| (*actual == point).then_some(*action))
            .collect::<Vec<_>>();
        if exit.point() != point || exit.actions() != expected || actual != expected {
            return Err(missing());
        }
    }
    Ok(())
}
