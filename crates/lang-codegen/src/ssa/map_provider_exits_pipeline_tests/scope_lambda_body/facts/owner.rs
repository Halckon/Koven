//! Lambda-local birth and current owner at the original header/exit epochs.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupCondition as C, CleanupOwnerValue as O, IterationPhiBoundary as B,
    IterationPhiIncomingKind as K,
};
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Input<'a> {
    pub site: Option<&'a crate::ssa::map_provider_exits_pipeline_tests::births::ast::Site>,
    pub birth: lang_frontend::ast::ExpressionId,
    pub birth_span: Span,
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn single(
    i: &Input<'_>,
    root: SymbolId,
    p: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
    drop: &DropFact,
) -> Result<(), Failure> {
    let site = i.site.as_ref().unwrap();
    let table = owned.cleanup_conditions();
    let h = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Header),
        "A2 exact current Header owner",
    )?;
    let x = one(
        p.closure_phis()
            .iter()
            .filter(|v| v.symbol() == root && v.boundary() == B::Exit),
        "A2 exact current Exit owner",
    )?;
    need(
        p.closure_phis().len() == 2
            && h.owner() != x.owner()
            && h.availability_selector() != x.availability_selector(),
        "A2 independent internal owner epochs",
        p.closure_phis(),
    )?;
    for phi in [h, x] {
        let selector = table
            .selector(phi.availability_selector())
            .ok_or_else(|| failure("A2 actual internal owner selector", phi))?;
        need(
            matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement,boundary,symbol,origin})if *statement==site.statement&&*boundary==phi.boundary()&&*symbol==root&&*origin==site.span)
                && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement,boundary,..}if statement==site.statement&&boundary==phi.boundary())
                && selector.origin() == site.span
                && selector.selection() == CleanupSelection::IterationPresence
                && selector.branch_count() == 2
                && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches})if *selector==phi.availability_selector()&&matches!(branches.as_slice(),[no,yes]if table.get(*no)==Some(&C::Never)&&table.get(*yes)==Some(&C::Always))),
            "A2 public owner/availability identity at actual loop boundary",
            (phi, selector),
        )?;
    }
    need(
        p.closure_phi_incomings().len() == 3,
        "A2 complete actual owner incoming set",
        p.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in [
        (K::Entry, DropPoint::AfterExpression(site.source), B::Header),
        (
            K::Fallthrough,
            DropPoint::AfterStatement(site.body),
            B::Header,
        ),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ] {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "A2 original owner incoming",
        )?;
        let phi = if boundary == B::Header { h } else { x };
        let binding = one(
            edge.bindings().iter(),
            "A2 sole internal owner incoming binding",
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
                && binding.presence_source() == IterationPhiPresenceSource::StaticConditions,
            "A2 complete actual source-owner incoming payload",
            edge,
        )?;
        if drop.point() == point {
            need(
                binding.values().is_empty()
                    && table.get(binding.available_when()) == Some(&C::Never),
                "A2 exhausted current Header was dropped and Exit is absent",
                edge,
            )?;
        } else {
            let value = one(
                binding.values().iter(),
                "A2 exact current live incoming value",
            )?;
            need(
                value.condition() == binding.available_when()
                    && if kind == K::Entry {
                        table.get(binding.available_when()) == Some(&C::Always)
                            && matches!(table.owner_value(value.source()),Some(O::Expression{expression,origin})if *expression==i.birth&&*origin==i.birth_span)
                    } else {
                        value.source() == h.owner()
                            && binding.available_when() == h.availability_condition()
                    },
                "A2 internal make birth or current Header owner handoff",
                edge,
            )?;
        }
    }
    need(
        drop.owner() == Some(h.owner())
            && drop.condition() == Some(h.availability_condition())
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none(),
        "A2 unique final cleanup consumes actual current Header epoch",
        drop,
    )
}
