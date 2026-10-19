//! Exact cleanup endpoints and complete Single owner boundary identities from v2 facts.
use super::*;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CleanupSelection, CleanupSelectorSource,
        CompilationUnitOwnership, DropPoint, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, IterationPhiPresenceSource, UnitDropPoint, UnitDropTarget,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_value_births::owner_update_observer)
struct Proof
{
    pub old_origin: Option<Span>,
    pub final_origin: Span,
    pub flag_in_provider: bool,
}
fn origin(c: &Case, p: &ParsedFile, i: &Input, unit: bool) -> Span {
    if c.code == 11 || c.code == 10 && (unit || !c.provider) {
        span(p, i.assignment.unwrap().0)
    } else {
        i.source
    }
}
pub(super) fn single(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &NameResolution,
    o: &OwnershipCheckedFile,
    root: SymbolId,
) -> Result<Proof, Failure> {
    if let (Some(marker), Some((statement, _))) = (i.flag, i.site) {
        let flag = super::local(n, marker)?;
        let (_, condition) = i
            .conditional
            .ok_or_else(|| failure("update Single prior flag condition AST", i))?;
        let references = n
            .references()
            .iter()
            .filter(|r| r.target() == &ReferenceTarget::Symbol(flag))
            .map(|r| r.span())
            .collect::<Vec<_>>();
        need(
            references.as_slice() == [span(p, condition)]
                && span(p, condition).end()
                    < p.ast().statements().get(statement).unwrap().span().start(),
            "update Single flag has only its exact earlier If condition use before the provider",
            references,
        )?;
    }
    for (receiver, clone) in clones(p, i)? {
        let drop = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Temporary(receiver)),
            "update exact original clone receiver temporary Drop",
        )?;
        need(
            drop.point() == DropPoint::CallReturn(clone)
                && drop.value_origin() == span(p, receiver)
                && drop.condition().is_none()
                && matches!(drop.owner().and_then(|v|o.cleanup_conditions().owner_value(v)),Some(O::Expression{expression,origin})if *expression==receiver&&*origin==span(p,receiver)),
            "update clone receiver cleanup retains its exact original expression/CallReturn",
            drop,
        )?;
    }
    let drops = o
        .drops()
        .iter()
        .filter(|d| d.target() == DropTarget::Named(root))
        .collect::<Vec<_>>();
    need(
        drops.len() == if c.code == 9 { 1 } else { 2 },
        "update exact Single source epoch obligation count",
        &drops,
    )?;
    let final_drop = one(
        drops
            .iter()
            .copied()
            .filter(|d| d.point() == DropPoint::CallReturn(i.observes.last().unwrap().0)),
        "update exact Single last observe CallReturn owner Drop",
    )?;
    need(
        final_drop.value_origin() == origin(c, p, i, false),
        "update Single final epoch original marker/assignment origin",
        final_drop,
    )?;
    let old = if c.code == 9 {
        None
    } else {
        let point = match c.code {
            10 => DropPoint::AfterStatement(super::super::ast::declaration(p, i.source)?),
            11 => {
                if let Some((statement, _)) = i.site {
                    DropPoint::LoopExit(statement)
                } else {
                    DropPoint::CallReturn(i.observes[0].0)
                }
            }
            _ => unreachable!(),
        };
        let old = one(
            drops.iter().copied().filter(|d| d.point() == point),
            "update exact Single old epoch endpoint",
        )?;
        need(
            old.value_origin() == i.source,
            "update Single old epoch retains actual source declaration origin",
            old,
        )?;
        Some(old)
    };
    if let Some((statement, _)) = i.site {
        let site = super::super::ast::site(p, i)?;
        let plan = o.map_iteration(statement).ok_or_else(|| {
            failure(
                "update same-analysis complete Single provider plan",
                statement,
            )
        })?;
        let mut roots = vec![(
            root,
            if c.code == 9 {
                i.births[0].expression
            } else {
                i.births[1].expression
            },
            c.code != 11,
        )];
        if c.code == 11 {
            roots.push((
                super::local(n, i.locals[0].marker)?,
                i.births[0].expression,
                true,
            ));
        }
        need(
            plan.closure_phis().len() == roots.len() * 2
                && plan.closure_phi_incomings().len() == 3
                && plan.capture_graph().nodes().is_empty(),
            "update complete exact source/independent-next Header/Exit graph",
            plan,
        )?;
        for (symbol, birth, alive) in roots {
            let h = one(
                plan.closure_phis()
                    .iter()
                    .filter(|v| v.symbol() == symbol && v.boundary() == B::Header),
                "update actual original Header owner",
            )?;
            let x = one(
                plan.closure_phis()
                    .iter()
                    .filter(|v| v.symbol() == symbol && v.boundary() == B::Exit),
                "update actual original Exit owner",
            )?;
            for phi in [h, x] {
                let slot = plan
                    .closure_phis()
                    .iter()
                    .filter(|v| v.boundary() == phi.boundary())
                    .position(|v| v.symbol() == symbol && v.owner() == phi.owner())
                    .ok_or_else(|| failure("update actual ordered boundary phi slot", phi))?;
                let table = o.cleanup_conditions();
                let selector = table.selector(phi.availability_selector()).ok_or_else(|| {
                    failure("update original boundary availability selector", phi)
                })?;
                need(
                    phi.root_nodes().is_empty()
                        && phi.capture_layout().is_empty()
                        && phi.origins().is_empty()
                        && matches!(table.owner_value(phi.owner()),Some(O::IterationPhi{statement:s,symbol:r,boundary,origin})if *s==statement&&*r==symbol&&*boundary==phi.boundary()&&*origin==site.span)
                        && matches!(selector.source(),CleanupSelectorSource::IterationPhi{statement:s,boundary,slot:actual_slot}if s==statement&&boundary==phi.boundary()&&actual_slot==slot)
                        && selector.origin() == site.span
                        && selector.selection() == CleanupSelection::IterationPresence
                        && selector.branch_count() == 2
                        && matches!(table.get(phi.availability_condition()),Some(C::Choice{selector,branches})if *selector==phi.availability_selector()&&matches!(branches.as_slice(),[no,yes]if table.get(*no)==Some(&C::Never)&&table.get(*yes)==Some(&C::Always))),
                    "update actual owner value and complete saved availability Choice",
                    (phi, selector),
                )?;
            }
            need(
                h.owner() != x.owner() && h.availability_selector() != x.availability_selector(),
                "update independent Header/Exit owner and availability IDs",
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
                    "update exact actual full incoming boundary",
                )?;
                let binding = one(
                    incoming
                        .bindings()
                        .iter()
                        .filter(|b| b.target() == target.owner()),
                    "update original actual incoming owner column",
                )?;
                let table = o.cleanup_conditions();
                need(
                    incoming.bindings().len() == plan.closure_phis().len() / 2
                        && incoming.point() == point
                        && incoming.boundary() == target.boundary()
                        && table.get(incoming.condition()) == Some(&C::Always)
                        && binding.availability_selector() == target.availability_selector()
                        && binding.capture_slots_to_clear().is_empty()
                        && binding.root_sources().is_empty()
                        && binding.selector_writes().is_empty()
                        && binding.origins().is_empty()
                        && binding.presence_source()
                            == IterationPhiPresenceSource::StaticConditions,
                    "update complete incoming identity/selector/empty capture layout",
                    (incoming, binding),
                )?;
                if kind == K::Exhaustion && !alive {
                    need(
                        binding.values().is_empty()
                            && table.get(binding.available_when()) == Some(&C::Never),
                        "update consumed old source Exit is explicitly absent",
                        binding,
                    )?;
                } else {
                    let value = one(
                        binding.values().iter(),
                        "update live original owner incoming",
                    )?;
                    need(
                        value.condition() == binding.available_when()
                            && if kind == K::Entry {
                                table.get(binding.available_when()) == Some(&C::Always)
                                    && matches!(table.owner_value(value.source()),Some(O::Expression{expression,origin})if *expression==birth&&*origin==span(p,birth))
                            } else {
                                value.source() == h.owner()
                                    && binding.available_when() == h.availability_condition()
                            },
                        "update all incoming owner origins are exact current Call/Header",
                        binding,
                    )?;
                }
            }
            let consumed = if c.code == 11 && symbol == root {
                old.unwrap()
            } else {
                final_drop
            };
            let expected = if c.code == 11 && symbol == root { h } else { x };
            need(
                consumed.owner() == Some(expected.owner())
                    && consumed.condition() == Some(expected.availability_condition()),
                "update Drop selects its exact current Header/Exit owner and saved condition",
                consumed,
            )?;
        }
    } else {
        for (drop, birth) in [
            (
                old,
                if c.code == 11 {
                    i.births.last().unwrap().expression
                } else {
                    i.births[0].expression
                },
            ),
            (
                Some(final_drop),
                if c.code == 10 {
                    i.births[1].expression
                } else {
                    i.births[0].expression
                },
            ),
        ] {
            if let Some(drop) = drop {
                need(
                    drop.condition().is_none()
                        && matches!(drop.owner().and_then(|v|o.cleanup_conditions().owner_value(v)),Some(O::Expression{expression,origin})if *expression==birth&&*origin==span(p,birth)),
                    "update control cleanup uses its actual original epoch Call owner",
                    drop,
                )?;
            }
        }
    }
    Ok(Proof {
        old_origin: old.map(|v| v.value_origin()),
        final_origin: final_drop.value_origin(),
        flag_in_provider: false,
    })
}
pub(super) fn unit(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    o: &CompilationUnitOwnership,
    root: UnitSymbolId,
) -> Result<Proof, Failure> {
    let e = |id| UnitExpressionId::new(root.source_unit(), id);
    for (receiver, clone) in clones(p, i)? {
        let drop = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(e(receiver))),
            "update qualified clone receiver temporary Drop",
        )?;
        need(
            drop.point() == UnitDropPoint::CallReturn(e(clone))
                && drop.value_origin() == span(p, receiver),
            "update qualified clone receiver exact original CallReturn/origin",
            drop,
        )?;
    }
    let drops = o
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    need(
        drops.len() == if c.code == 9 { 1 } else { 2 },
        "update exact Unit source epoch obligation count",
        &drops,
    )?;
    let last = one(
        drops
            .iter()
            .copied()
            .filter(|d| d.point() == UnitDropPoint::CallReturn(e(i.observes.last().unwrap().0))),
        "update Unit exact last observe CallReturn source Drop",
    )?;
    need(
        last.value_origin() == origin(c, p, i, true),
        "update Unit current assignment/declaration Drop origin",
        last,
    )?;
    let old = if c.code == 9 {
        None
    } else {
        let point = match c.code {
            7 | 8 => UnitDropPoint::AfterExpression(e(i.births[1].expression)),
            10 => UnitDropPoint::AfterStatement(UnitStatementId::new(
                root.source_unit(),
                super::super::ast::declaration(p, i.source)?,
            )),
            11 => {
                if let Some((statement, _)) = i.site {
                    UnitDropPoint::LoopExit(UnitStatementId::new(root.source_unit(), statement))
                } else {
                    UnitDropPoint::CallReturn(e(i.observes[0].0))
                }
            }
            _ => unreachable!(),
        };
        let old = one(
            drops.iter().copied().filter(|d| d.point() == point),
            "update Unit exact original replacement/old source endpoint",
        )?;
        need(
            old.value_origin() == i.source,
            "update Unit old epoch original declaration marker",
            old,
        )?;
        Some(old)
    };
    Ok(Proof {
        old_origin: old.map(|v| v.value_origin()),
        final_origin: last.value_origin(),
        flag_in_provider: i.flag.is_some(),
    })
}
fn clones(
    p: &ParsedFile,
    i: &Input,
) -> Result<
    Vec<(
        lang_frontend::ast::ExpressionId,
        lang_frontend::ast::ExpressionId,
    )>,
    Failure,
> {
    let Some(put) = i.put else {
        return Ok(vec![]);
    };
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        unreachable!()
    };
    let receivers = super::super::ast::clone_receivers(p, i)?;
    Ok(receivers
        .into_iter()
        .zip(arguments.iter().map(|a| a.value))
        .collect())
}
