//! Full owner/availability and current RHS identity for each independent iteration epoch.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupCondition as C, CleanupSelection, CleanupSelectorSource, DropFact,
    IterationPhiIncomingKind as K, IterationPhiPresenceSource, MapIterationOwnershipPlan,
};
pub(super) fn check(
    site: &ast::Site,
    l: &ast::Loop,
    root: SymbolId,
    p: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
    parsed: &ParsedFile,
    drop: &DropFact,
    resource: bool,
) -> Result<(), Failure> {
    let table = owned.cleanup_conditions();
    need(
        p.closure_phis().len() == 2
            && p.closure_phi_incomings().len() == 3
            && p.capture_graph().nodes().is_empty(),
        "epoch exact public owner graph",
        p,
    )?;
    let h = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Header),
        "epoch actual Header",
    )?;
    let x = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Exit),
        "epoch actual Exit",
    )?;
    need(
        h.owner() != x.owner() && h.availability_selector() != x.availability_selector(),
        "epoch independent boundary owner/selector",
        (h, x),
    )?;
    for phi in [h, x] {
        let selector = table
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("epoch actual selector", phi))?;
        need(
            matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement,symbol,boundary,origin}) if *statement==l.statement && *symbol==root && *boundary==phi.boundary() && *origin==l.span)
                && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..} if statement==l.statement && boundary==phi.boundary())
                && selector.origin() == l.span
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.branch_count() == 2
                && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches}) if *selector==phi.availability_selector() && matches!(branches.as_slice(),[no,yes] if table.get(*no)==Some(&C::Never) && table.get(*yes)==Some(&C::Always))),
            "epoch full current owner and Choice",
            (phi, selector),
        )?;
    }
    let birth = if l.epoch == 0 { site.birth } else { site.rhs };
    for (kind, phi, point) in [
        (K::Entry, h, DropPoint::AfterExpression(l.source)),
        (K::Fallthrough, h, DropPoint::AfterStatement(l.body)),
        (K::Exhaustion, x, DropPoint::LoopExit(l.statement)),
    ] {
        let incoming = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "epoch actual unique incoming",
        )?;
        let binding = one(
            incoming.bindings().iter(),
            "epoch complete incoming binding",
        )?;
        need(
            incoming.point() == point
                && incoming.boundary() == phi.boundary()
                && table.get(incoming.condition()) == Some(&C::Always)
                && binding.target() == phi.owner()
                && binding.availability_selector() == phi.availability_selector()
                && binding.capture_slots_to_clear().is_empty()
                && binding.root_sources().is_empty()
                && binding.selector_writes().is_empty()
                && binding.origins().is_empty()
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions,
            "epoch complete incoming target, selector and non-capture layout",
            incoming,
        )?;
        // Pure source cleanup consumes Header at LoopExit. Its Exit explicitly
        // receives absence; Resource source instead transports its live owner.
        if !resource && kind == K::Exhaustion {
            need(
                binding.values().is_empty()
                    && table.get(binding.available_when()) == Some(&C::Never),
                "Pure exhaustion writes exact absent Exit after Header Drop",
                binding,
            )?;
            continue;
        }
        let value = one(binding.values().iter(), "epoch actual live incoming value")?;
        need(
            value.condition() == binding.available_when()
                && if kind == K::Entry {
                    table.get(binding.available_when()) == Some(&C::Always)
                        && matches!(table.owner_value(value.source()),Some(O::Expression{expression,origin}) if *expression==birth && *origin==ast::expression(parsed,birth))
                } else {
                    value.source() == h.owner()
                        && binding.available_when() == h.availability_condition()
                },
            "every live epoch incoming derives current complete Call RHS, never declaration history",
            incoming,
        )?;
    }
    let expected = if resource { x } else { h };
    need(
        drop.owner() == Some(expected.owner())
            && drop.condition() == Some(expected.availability_condition()),
        "Drop consumes correct Pure Header or Resource Exit epoch",
        drop,
    )?;
    // The Name may move to alias, but the actual birth must remain the other() expression.
    Ok(())
}
