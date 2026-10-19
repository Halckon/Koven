//! Exact birth, live Exit-to-Header transport and final Pure ASAP / Resource lexical owner obligation.
use super::*;
use lang_frontend::name_resolution::SymbolId;
use lang_frontend::ownership_checking::{
    CleanupCondition as C, CleanupOwnerValue as O, IterationPhiBoundary as B,
    IterationPhiIncomingKind as K,
};
pub(super) fn single(
    input: &ast::Input,
    root: SymbolId,
    p: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
    drop: &DropFact,
) -> Result<(), Failure> {
    let site = input
        .site
        .as_ref()
        .ok_or_else(|| failure("birth original provider site", input))?;
    let table = owned.cleanup_conditions();
    let h = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Header),
        "birth actual Header owner",
    )?;
    let x = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Exit),
        "birth actual Exit owner",
    )?;
    need(
        p.closure_phis().len() == 2
            && h.owner() != x.owner()
            && h.availability_selector() != x.availability_selector(),
        "birth distinct original owner boundaries",
        p.closure_phis(),
    )?;
    for phi in [h, x] {
        let selector = table
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("birth original selector", phi))?;
        need(
            matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement,boundary,symbol,origin}) if *statement==site.statement && *boundary==phi.boundary() && *symbol==root && *origin==site.span)
                && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..} if statement==site.statement && boundary==phi.boundary())
                && selector.origin() == site.span
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.branch_count() == 2
                && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches}) if *selector==phi.availability_selector() && matches!(branches.as_slice(),[no,yes] if table.get(*no)==Some(&C::Never) && table.get(*yes)==Some(&C::Always))),
            "birth full public owner/Choice identity",
            (phi, selector),
        )?;
    }
    let incoming = K::Fallthrough;
    need(
        p.closure_phi_incomings().len() == 3,
        "birth full three incoming set",
        p.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in [
        (K::Entry, DropPoint::AfterExpression(site.source), B::Header),
        (
            incoming,
            DropPoint::AfterStatement(site.body),
            if matches!(incoming, K::Break(_)) {
                B::Exit
            } else {
                B::Header
            },
        ),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ] {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "birth original incoming",
        )?;
        let phi = if boundary == B::Header { h } else { x };
        let binding = one(edge.bindings().iter(), "birth sole owner binding")?;
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
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions,
            "birth exact original Call/previous Exit/current Header handoff",
            edge,
        )?;
        if drop.point() == point {
            need(
                binding.values().is_empty()
                    && table.get(binding.available_when()) == Some(&C::Never),
                "birth cleaned Header owner writes exactly absent Exit",
                edge,
            )?;
        } else {
            let value = one(binding.values().iter(), "birth exact live incoming value")?;
            need(
                value.condition() == binding.available_when()
                    && if kind == K::Entry {
                        table.get(binding.available_when()) == Some(&C::Always)
                            && matches!(table.owner_value(value.source()),Some(O::Expression{expression,origin})if *expression==input.birth&&*origin==input.birth_span)
                    } else {
                        value.source() == h.owner()
                            && binding.available_when() == h.availability_condition()
                    },
                "birth original Call birth or current Header incoming",
                edge,
            )?;
        }
    }
    need(
        drop.owner()
            == Some(if drop.point() == DropPoint::LoopExit(site.statement) {
                h.owner()
            } else {
                x.owner()
            })
            && drop.condition()
                == Some(if drop.point() == DropPoint::LoopExit(site.statement) {
                    h.availability_condition()
                } else {
                    x.availability_condition()
                })
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none(),
        "birth final Drop consumes exact current Header/Exit epoch",
        drop,
    )?;
    Ok(())
}
