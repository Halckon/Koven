//! All public single-file owner incoming slots, including live and absent edges.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupCondition as C, CleanupOwnerValue as O, DropPoint, IterationPhiBoundary as B,
    IterationPhiIncomingKind as K, IterationPhiPresenceSource, MapIterationOwnershipPlan,
    OwnershipCheckedFile,
};
pub(super) fn check(
    parsed: &ParsedFile,
    site: &AstSite,
    p: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
    names: &lang_frontend::name_resolution::NameResolution,
) -> Result<(), Failure> {
    let table = owned.cleanup_conditions();
    let mut expected = vec![
        (K::Entry, DropPoint::AfterExpression(site.source), B::Header),
        (
            K::Fallthrough,
            DropPoint::AfterStatement(site.body),
            B::Header,
        ),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ];
    for exit in p.exits() {
        if let lang_frontend::ownership_checking::IterationExitKind::Break(id) = exit.kind() {
            expected.push((K::Break(id), DropPoint::ControlTransfer(id), B::Exit));
        }
    }
    need(
        p.closure_phi_incomings().len() == expected.len(),
        "stack complete original owner incoming edge set",
        p.closure_phi_incomings(),
    )?;
    for (kind, point, boundary) in expected {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|v| v.kind() == kind),
            "stack unique original owner incoming",
        )?;
        conditions::path(parsed, site, edge, owned)?;
        let phis = p
            .closure_phis()
            .iter()
            .filter(|phi| phi.boundary() == boundary)
            .collect::<Vec<_>>();
        need(
            edge.point() == point
                && edge.boundary() == boundary
                && table.get(edge.condition()).is_some()
                && edge.bindings().len() == phis.len(),
            "stack owner incoming exact point/boundary/full binding arity",
            edge,
        )?;
        for phi in phis {
            let binding = one(
                edge.bindings().iter().filter(|b| b.target() == phi.owner()),
                "stack actual owner incoming target column",
            )?;
            need(
                binding.availability_selector() == phi.availability_selector()
                    && table.get(binding.available_when()).is_some()
                    && binding.capture_slots_to_clear().is_empty()
                    && binding.root_sources().is_empty()
                    && binding.selector_writes().is_empty()
                    && binding.origins().is_empty()
                    && binding.presence_source() == IterationPhiPresenceSource::StaticConditions
                    && phi.root_nodes().is_empty()
                    && phi.capture_layout().is_empty()
                    && phi.origins().is_empty(),
                "stack ordinary owner complete no-capture incoming layout",
                (phi, binding),
            )?;
            if table.get(binding.available_when()) == Some(&C::Never) {
                let header = one(
                    p.closure_phis()
                        .iter()
                        .filter(|h| h.boundary() == B::Header && h.symbol() == phi.symbol()),
                    "stack absent Exit original Header owner",
                )?;
                let dropped=p.exits().iter().filter(|exit|exit.point()==point).flat_map(|exit|exit.actions()).any(|action|{
                    matches!(action,lang_frontend::ownership_checking::IterationCleanupAction::Drop(drop)
                        if drop.target()==lang_frontend::ownership_checking::DropTarget::Named(phi.symbol())
                            && drop.owner()==Some(header.owner())
                            && drop.condition()==Some(header.availability_condition())
                            && owned.drops().contains(drop))
                });
                need(
                    binding.values().is_empty() && kind == K::Exhaustion && dropped,
                    "stack absent Exit has no value and consumes its exact current Header once",
                    (edge, binding, header),
                )?;
                continue;
            }
            let value = one(
                binding.values().iter(),
                "stack one current live owner source",
            )?;
            need(
                value.condition() == binding.available_when(),
                "stack owner source condition exact",
                binding,
            )?;
            let source = table
                .owner_value(value.source())
                .ok_or_else(|| failure("stack real incoming source owner", value))?;
            if kind == K::Entry {
                let declared = one(
                    names.symbols().iter().filter(|s| s.id() == phi.symbol()),
                    "stack Entry actual owner declaration",
                )?;
                let actual = match source {
                    O::Parameter{symbol,origin}=>*symbol==phi.symbol()&&*origin==declared.span()&&parsed.roots().iter().any(|id|parsed.ast().items().get(*id).is_ok_and(|n|n.span()==site.function&&facts_parameter(n.payload(),*origin))),
                    O::Expression{expression,origin}=>parsed.ast().expressions().get(*expression).is_ok_and(|n|n.span()==*origin)
                        && parsed.ast().items().iter().any(|(_,n)|matches!(n.payload(),Item::Variable{name:NameMarker::Present(marker),initializer,..} if *marker==declared.span()&&*initializer==*expression)),
                    O::IterationPhi{statement,symbol,boundary,origin}=>{
                        let parent=owned.map_iterations().iter().filter_map(|parent|{
                            let st=parent.descriptor().statement();
                            if st==site.statement{return None;}
                            let node=parsed.ast().statements().get(st).ok()?;
                            let Statement::For{body,..}=node.payload()else{return None;};
                            let body=parsed.ast().statements().get(*body).ok()?.span();
                            if !inside(site.span,body){return None;}
                            parent.closure_phis().iter().find(|h|h.symbol()==phi.symbol()&&h.boundary()==B::Header).map(|h|(node.span(),st,h))
                        }).min_by_key(|(span,_,_)|span.end()-span.start());
                        parent.is_some_and(|(span,st,h)|*symbol==phi.symbol()&&*boundary==B::Header&&*statement==st&&*origin==span&&value.source()==h.owner()&&binding.available_when()==h.availability_condition())
                    },
                    _=>false,
                };
                need(
                    actual,
                    "stack Entry owner from actual parameter/expression/enclosing current phi",
                    (phi, source),
                )?;
            } else {
                let header = one(
                    p.closure_phis()
                        .iter()
                        .filter(|h| h.boundary() == B::Header && h.symbol() == phi.symbol()),
                    "stack exact original current Header owner",
                )?;
                // A completed child loop replaces the parent's current root
                // with its exact Exit owner before the parent's backedge. The
                // provider's HasNext-false edge still reads its own Header.
                let cutoff = match kind {
                    K::Fallthrough => Some(
                        parsed
                            .ast()
                            .statements()
                            .get(site.body)
                            .unwrap()
                            .span()
                            .end(),
                    ),
                    K::Break(id) => {
                        Some(parsed.ast().expressions().get(id).unwrap().span().start())
                    }
                    _ => None,
                };
                let child = cutoff.and_then(|cutoff| {
                    owned
                        .map_iterations()
                        .iter()
                        .filter_map(|child| {
                            let span = parsed
                                .ast()
                                .statements()
                                .get(child.descriptor().statement())
                                .ok()?
                                .span();
                            if child.descriptor().statement() == site.statement
                                || !inside(span, site.body_span)
                                || span.end() > cutoff
                            {
                                return None;
                            }
                            child
                                .closure_phis()
                                .iter()
                                .find(|x| x.boundary() == B::Exit && x.symbol() == phi.symbol())
                                .map(|x| (span.end(), x))
                        })
                        .max_by_key(|(end, _)| *end)
                        .map(|(_, x)| x)
                });
                let current = child.unwrap_or(header);
                need(
                    value.source() == current.owner(),
                    "stack every live incoming transports exact lexical current Header/completed-child Exit owner",
                    (edge, binding, current),
                )?;
                conditions::conjunction(
                    parsed,
                    owned,
                    binding.available_when(),
                    edge.condition(),
                    current.availability_condition(),
                )?;
            }
        }
    }
    Ok(())
}
fn facts_parameter(item: &Item, marker: Span) -> bool {
    matches!(item,Item::Function{parameters,..} if parameters.iter().any(|p|p.name==NameMarker::Present(marker)))
}
