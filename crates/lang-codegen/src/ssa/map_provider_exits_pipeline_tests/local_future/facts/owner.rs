//! Exact birth, live Exit-to-Header transport and final Pure ASAP owner obligation.
use super::*;
pub(super) fn check(
    site: &Scanner,
    loop_site: &Loop,
    root: SymbolId,
    previous: Option<(CleanupOwnerValueId, CleanupConditionId)>,
    p: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
) -> Result<(CleanupOwnerValueId, CleanupConditionId), Failure> {
    let table = owned.cleanup_conditions();
    let h = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Header),
        "local future actual Header owner",
    )?;
    let x = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Exit),
        "local future actual Exit owner",
    )?;
    need(
        p.closure_phis().len() == 2
            && h.owner() != x.owner()
            && h.availability_selector() != x.availability_selector(),
        "local future distinct original owner boundaries",
        p.closure_phis(),
    )?;
    for phi in [h, x] {
        let selector = table
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("local future original selector", phi))?;
        need(
            matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement,boundary,symbol,origin}) if *statement==loop_site.statement && *boundary==phi.boundary() && *symbol==root && *origin==loop_site.span)
                && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..} if statement==loop_site.statement && boundary==phi.boundary())
                && selector.origin() == loop_site.span
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.branch_count() == 2
                && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches}) if *selector==phi.availability_selector() && matches!(branches.as_slice(),[no,yes] if table.get(*no)==Some(&C::Never) && table.get(*yes)==Some(&C::Always))),
            "local future full public owner/Choice identity",
            (phi, selector),
        )?;
    }
    let incoming = match loop_site.kind {
        E::Fallthrough => K::Fallthrough,
        E::Continue(e) => K::Continue(e),
        E::Break(e) => K::Break(e),
        other => return Err(failure("local future original incoming kind", other)),
    };
    need(
        p.closure_phi_incomings().len() == 3,
        "local future full three incoming set",
        p.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in [
        (
            K::Entry,
            DropPoint::AfterExpression(loop_site.source),
            B::Header,
        ),
        (
            incoming,
            loop_site.point,
            if matches!(incoming, K::Break(_)) {
                B::Exit
            } else {
                B::Header
            },
        ),
        (
            K::Exhaustion,
            DropPoint::LoopExit(loop_site.statement),
            B::Exit,
        ),
    ] {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "local future original incoming",
        )?;
        let phi = if boundary == B::Header { h } else { x };
        let binding = one(edge.bindings().iter(), "local future sole owner binding")?;
        let value = one(
            binding.values().iter(),
            "local future every incoming remains live",
        )?;
        let provenance = if kind == K::Entry {
            if let Some((owner, available)) = previous {
                value.source() == owner && binding.available_when() == available
            } else {
                table.get(binding.available_when()) == Some(&C::Always)
                    && matches!(table.owner_value(value.source()),Some(O::Expression{expression,origin}) if *expression==site.birth.initializer && *origin==site.birth_span)
            }
        } else {
            value.source() == h.owner() && binding.available_when() == h.availability_condition()
        };
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
                && provenance,
            "local future exact original Call/previous Exit/current Header handoff",
            (edge, previous),
        )?;
    }
    Ok((x.owner(), x.availability_condition()))
}
