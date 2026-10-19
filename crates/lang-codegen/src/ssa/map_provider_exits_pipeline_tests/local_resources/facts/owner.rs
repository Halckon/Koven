//! Resource owners remain live at Exit until the actual scanner's lexical Return.
use super::*;

pub(super) fn lexical(site: &Site, parsed: &ParsedFile) -> Result<DropPoint, Failure> {
    one(
        parsed.ast().expressions().iter().filter_map(|(id, node)| {
            (matches!(node.payload(), Expression::Return { .. })
                && within(site.scan, node.span())
                && node.span().start() > site.span.end())
            .then_some(DropPoint::ControlTransfer(id))
        }),
        "local Resource actual lexical Return",
    )
}

pub(super) fn single(
    site: &Site,
    parsed: &ParsedFile,
    root: SymbolId,
    p: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let lexical = lexical(site, parsed)?;
    let table = owned.cleanup_conditions();
    let h = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Header),
        "Resource local actual Header",
    )?;
    let x = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Exit),
        "Resource local actual Exit",
    )?;
    need(
        p.closure_phis().len() == 2
            && h.owner() != x.owner()
            && h.availability_selector() != x.availability_selector(),
        "Resource local independent owner boundaries",
        p.closure_phis(),
    )?;
    for phi in [h, x] {
        let selector = table
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("actual Resource selector", phi))?;
        need(
            matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement,boundary,symbol,origin}) if *statement==site.statement && *boundary==phi.boundary() && *symbol==root && *origin==site.span)
                && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..} if statement==site.statement && boundary==phi.boundary())
                && selector.origin() == site.span
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.branch_count() == 2
                && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches}) if *selector==phi.availability_selector() && matches!(branches.as_slice(),[no,yes] if table.get(*no)==Some(&C::Never) && table.get(*yes)==Some(&C::Always))),
            "Resource local full owner/Choice identity",
            (phi, selector),
        )?;
    }
    let drops = owned
        .drops()
        .iter()
        .filter(|d| d.target() == DropTarget::Named(root))
        .collect::<Vec<_>>();
    let returning = matches!(site.kind, E::Return(_));
    need(
        drops.len() == if returning { 2 } else { 1 }
            && drops.iter().filter(|d| d.point() == lexical).count() == 1
            && (!returning || drops.iter().filter(|d| d.point() == site.point).count() == 1)
            && drops.iter().all(|d| {
                d.value_origin() == site.root.marker
                    && d.capture_slot().is_none()
                    && d.instance_address().is_none()
                    && if d.point() == lexical {
                        d.owner() == Some(x.owner())
                            && d.condition() == Some(x.availability_condition())
                    } else {
                        returning
                            && d.point() == site.point
                            && d.owner() == Some(h.owner())
                            && d.condition() == Some(h.availability_condition())
                    }
            }),
        "Resource local lexical Exit Drop and optional actual Return Header Drop",
        &drops,
    )?;
    let mut edges = vec![
        (K::Entry, DropPoint::AfterExpression(site.source), B::Header),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ];
    match site.kind {
        E::Fallthrough => edges.push((K::Fallthrough, site.point, B::Header)),
        E::Continue(e) => edges.push((K::Continue(e), site.point, B::Header)),
        E::Break(e) => edges.push((K::Break(e), site.point, B::Exit)),
        E::Return(_) => {}
        _ => return Err(failure("actual Resource exit", site.kind)),
    }
    need(
        p.closure_phi_incomings().len() == edges.len(),
        "Resource local complete incoming set, Return has no merge",
        p.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in edges {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "Resource local original incoming",
        )?;
        let phi = if boundary == B::Header { h } else { x };
        let binding = one(edge.bindings().iter(), "Resource local one owner binding")?;
        let value = one(
            binding.values().iter(),
            "Resource local must remain live at incoming",
        )?;
        need(
            edge.point() == point
                && edge.boundary() == boundary
                && table.get(edge.condition()) == Some(&C::Always)
                && binding.target() == phi.owner()
                && binding.availability_selector() == phi.availability_selector()
                && binding.capture_slots_to_clear().is_empty()
                && binding.root_sources().is_empty()
                && binding.selector_writes().is_empty()
                && binding.origins().is_empty()
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions
                && value.condition() == binding.available_when()
                && if kind == K::Entry {
                    table.get(binding.available_when()) == Some(&C::Always)
                        && matches!(table.owner_value(value.source()),Some(O::Expression{expression,origin}) if *expression==site.birth.initializer && *origin==site.birth_span)
                } else {
                    value.source() == h.owner()
                        && binding.available_when() == h.availability_condition()
                },
            "Resource local exact original factory birth/current Header to live Exit",
            edge,
        )?;
    }
    let scope = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(at, action)| (*at == lexical).then_some(*action))
        .collect::<Vec<_>>();
    let drop = one(
        drops.iter().copied().filter(|d| d.point() == lexical),
        "Resource local real scope Drop",
    )?;
    need(
        scope == [A::Drop(*drop)],
        "Resource local complete lexical cleanup is original Exit obligation",
        scope,
    )
}
