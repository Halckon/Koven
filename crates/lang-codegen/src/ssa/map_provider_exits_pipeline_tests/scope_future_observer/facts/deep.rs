//! Exact published cleanup endpoints and current owner identities, from the same analysis.
use super::*;
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CleanupSelection, CleanupSelectorSource,
        CompilationUnitOwnership, DropPoint, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, IterationPhiPresenceSource, UnitDropPoint, UnitDropTarget,
    },
    parser::Expression,
    type_checking::UnitExpressionId,
};
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_future_observer) struct Proof {
    pub final_origin: Option<Span>,
    pub caller_origin: Option<Span>,
}
fn final_marker(c: &Case, i: &Input) -> Result<Option<Span>, Failure> {
    match c.code {
        19 => Ok(None),
        20 => Ok(Some(
            i.successor
                .ok_or_else(|| failure("D actual successor marker", i))?
                .0,
        )),
        21 => Ok(Some(i.source)),
        _ => unreachable!(),
    }
}
pub(super) fn clones(
    p: &ParsedFile,
    i: &Input,
) -> Result<Vec<(ExpressionId, ExpressionId)>, Failure> {
    let Some(put) = i.put else {
        return Ok(vec![]);
    };
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        return Err(failure("D original put Call", put));
    };
    need(
        arguments.len() == 2,
        "D put exact owned key/value slots",
        arguments,
    )?;
    arguments
        .iter()
        .map(|arg| {
            let Expression::Call {
                callee, arguments, ..
            } = p.ast().expressions().get(arg.value).unwrap().payload()
            else {
                return Err(failure("D original clone Call", arg));
            };
            let Expression::Member { receiver, .. } =
                p.ast().expressions().get(*callee).unwrap().payload()
            else {
                return Err(failure("D actual clone receiver", callee));
            };
            need(
                arguments.is_empty(),
                "D clone zero actual arguments",
                arguments,
            )?;
            Ok((*receiver, arg.value))
        })
        .collect()
}
pub(super) fn single(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &NameResolution,
    o: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
    let root = symbol(n, i.source)?;
    for (receiver, clone) in clones(p, i)? {
        let d = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Temporary(receiver)),
            "D exact clone receiver obligation",
        )?;
        let effect = o
            .string_effect(clone)
            .ok_or_else(|| failure("D mandatory actual clone effect", clone))?;
        need(
            effect.receiver() == receiver
                && d.point() == DropPoint::CallReturn(clone)
                && d.value_origin() == ast::span(p, receiver)
                && d.condition().is_none()
                && matches!(d.owner().and_then(|v|o.cleanup_conditions().owner_value(v)), Some(O::Expression{expression,origin})if *expression==receiver&&*origin==ast::span(p,receiver)),
            "D independent clone receiver exact effect/CallReturn/owner",
            (effect, d),
        )?;
    }
    let final_origin = final_marker(c, i)?;
    let final_drop = if let Some(marker) = final_origin {
        let target = symbol(n, marker)?;
        let d = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Named(target)),
            "D unique final named obligation",
        )?;
        need(
            d.point() == DropPoint::CallReturn(i.observes.last().unwrap().0)
                && d.value_origin() == marker,
            "D actual final observe CallReturn and declaration origin",
            d,
        )?;
        Some(d)
    } else {
        None
    };
    let caller_origin = i.result;
    if let Some(marker) = caller_origin {
        let result = symbol(n, marker)?;
        let d = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Named(result)),
            "D exact caller returned-result Drop",
        )?;
        need(
            d.point() == DropPoint::CallReturn(i.entry_observe.unwrap().0)
                && d.value_origin() == marker
                && d.condition().is_none()
                && matches!(d.owner().and_then(|v|o.cleanup_conditions().owner_value(v)),Some(O::Expression{expression,origin})if *expression==i.scan_call&&*origin==ast::span(p,i.scan_call)),
            "D caller Drop owns actual scan Call result at final CallReturn",
            d,
        )?;
    }
    if let Some((statement, _)) = i.site {
        let site = ast::site(p, i)?;
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("D mandatory same-analysis owner plan", statement))?;
        need(
            plan.closure_phis().len() == 2
                && plan.closure_phi_incomings().len() == 3
                && plan.capture_graph().nodes().is_empty(),
            "D exact source Header/Exit and complete incoming family",
            plan,
        )?;
        let h = one(
            plan.closure_phis()
                .iter()
                .filter(|v| v.symbol() == root && v.boundary() == B::Header),
            "D original Header owner",
        )?;
        let x = one(
            plan.closure_phis()
                .iter()
                .filter(|v| v.symbol() == root && v.boundary() == B::Exit),
            "D original Exit owner",
        )?;
        let table = o.cleanup_conditions();
        for phi in [h, x] {
            let slot = plan
                .closure_phis()
                .iter()
                .filter(|v| v.boundary() == phi.boundary())
                .position(|v| v.symbol() == root && v.owner() == phi.owner())
                .ok_or_else(|| failure("D actual ordered boundary slot", phi))?;
            let selector = table
                .selector(phi.availability_selector())
                .ok_or_else(|| failure("D actual saved owner availability selector", phi))?;
            need(
                phi.root_nodes().is_empty()
                    && phi.capture_layout().is_empty()
                    && phi.origins().is_empty()
                    && matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement:s,boundary,symbol,origin})if *s==statement&&*boundary==phi.boundary()&&*symbol==root&&*origin==site.span)
                    && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement:s,boundary,slot:actual}if s==statement&&boundary==phi.boundary()&&actual==slot)
                    && selector.origin() == site.span
                    && selector.selection() == CleanupSelection::IterationPresence
                    && selector.branch_count() == 2
                    && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches})if *selector==phi.availability_selector()&&matches!(branches.as_slice(),[no,yes]if table.get(*no)==Some(&C::Never)&&table.get(*yes)==Some(&C::Always))),
                "D exact current owner/availability Choice source identity",
                (phi, selector),
            )?;
        }
        need(
            h.owner() != x.owner() && h.availability_selector() != x.availability_selector(),
            "D independent Header and Exit identities",
            (h, x),
        )?;
        for (kind, point, target) in [
            (K::Entry, DropPoint::AfterExpression(site.source), h),
            (K::Fallthrough, DropPoint::AfterStatement(site.body), h),
            (K::Exhaustion, DropPoint::LoopExit(statement), x),
        ] {
            let incoming = one(
                plan.closure_phi_incomings()
                    .iter()
                    .filter(|v| v.kind() == kind),
                "D exact complete published owner incoming",
            )?;
            let b = one(
                incoming.bindings().iter(),
                "D sole actual source incoming column",
            )?;
            let v = one(
                b.values().iter(),
                "D every normal incoming retains current owner",
            )?;
            need(
                incoming.point() == point
                    && incoming.boundary() == target.boundary()
                    && table.get(incoming.condition()) == Some(&C::Always)
                    && b.target() == target.owner()
                    && b.availability_selector() == target.availability_selector()
                    && b.capture_slots_to_clear().is_empty()
                    && b.root_sources().is_empty()
                    && b.selector_writes().is_empty()
                    && b.origins().is_empty()
                    && b.presence_source() == IterationPhiPresenceSource::StaticConditions
                    && v.condition() == b.available_when()
                    && if kind == K::Entry {
                        table.get(b.available_when()) == Some(&C::Always)
                            && if let Some(birth) = i.birth {
                                matches!(table.owner_value(v.source()),Some(O::Expression{expression,origin})if *expression==birth&&*origin==ast::span(p,birth))
                            } else {
                                matches!(table.owner_value(v.source()),Some(O::Parameter{symbol,origin})if *symbol==root&&*origin==i.source)
                            }
                    } else {
                        v.source() == h.owner() && b.available_when() == h.availability_condition()
                    },
                "D exact Entry original Parameter/Call then complete live Header/Exit transport",
                (incoming, b),
            )?;
        }
        if let Some(d) = final_drop {
            need(
                d.owner() == Some(x.owner()) && d.condition() == Some(x.availability_condition()),
                "D moved successor/updated source cleanup uses actual Exit owner and condition",
                d,
            )?;
        }
    } else if let Some(d) = final_drop {
        need(
            d.condition().is_none()
                && if let Some(birth) = i.birth {
                    matches!(d.owner().and_then(|v|o.cleanup_conditions().owner_value(v)),Some(O::Expression{expression,origin})if *expression==birth&&*origin==ast::span(p,birth))
                } else {
                    matches!(d.owner().and_then(|v|o.cleanup_conditions().owner_value(v)),Some(O::Parameter{symbol,origin})if *symbol==root&&*origin==i.source)
                },
            "D control Move/put preserves original Parameter or Call cleanup owner",
            d,
        )?;
    }
    Ok(Proof {
        final_origin,
        caller_origin,
    })
}
pub(super) fn unit(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let root = super::unit::symbol(n, i.source)?;
    let e = |id| UnitExpressionId::new(root.source_unit(), id);
    for (receiver, clone) in clones(p, i)? {
        let effect = o
            .string_effect(e(clone))
            .ok_or_else(|| failure("D qualified actual clone effect", e(clone)))?;
        let d = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(e(receiver))),
            "D qualified independent clone receiver Drop",
        )?;
        need(
            effect.receiver() == e(receiver)
                && d.point() == UnitDropPoint::CallReturn(e(clone))
                && d.value_origin() == ast::span(p, receiver),
            "D qualified clone receiver exact effect/CallReturn/origin",
            (effect, d),
        )?;
    }
    let final_origin = final_marker(c, i)?;
    for (marker, call) in final_origin
        .map(|m| (m, i.observes.last().unwrap().0))
        .into_iter()
        .chain(i.result.map(|m| (m, i.entry_observe.unwrap().0)))
    {
        let target = super::unit::symbol(n, marker)?;
        let d = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(target)),
            "D qualified sole final named Drop",
        )?;
        need(
            d.point() == UnitDropPoint::CallReturn(e(call)) && d.value_origin() == marker,
            "D qualified last actual observe exact Drop point and declaration origin",
            d,
        )?;
    }
    Ok(Proof {
        final_origin,
        caller_origin: i.result,
    })
}
