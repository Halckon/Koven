//! Public owner definitions, full phi incoming sets and dead predecessor obligations.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::super::ast::{Anchors, Input};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget, SymbolKind},
    ownership_checking::{
        CleanupCondition as C, CleanupConditionId, CleanupConditions, CleanupOwnerValue as V,
        CleanupSelection, CleanupSelectorId, CleanupSelectorSource, DropPoint as P, DropTarget,
        IterationCleanupAction as A, IterationExitKind as E, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, IterationPhiPresenceSource, LoanTarget,
        OwnershipCheckedFile,
    },
    type_checking::{CallableTarget, TypedFile},
};

pub(in super::super) fn query(
    input: &Input,
    anchors: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let conditions = owned.cleanup_conditions();
    for site in &anchors.sites {
        let root = one(
            names.symbols().iter().filter(|s| {
                s.span() == site.local.marker
                    && s.kind() == SymbolKind::Variable
                    && !s.is_synthetic()
            }),
            "local original declaration symbol",
        )?
        .id();
        let reference = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == site.source_span),
            "local original source reference",
        )?;
        need(
            matches!(reference.target(), ReferenceTarget::Symbol(s) if *s == root),
            "local declaration/reference independent root",
            reference,
        )?;
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("local same-analysis plan", site))?;
        need(
            matches!(p.source(), Some(LoanTarget::Place(v)) if v.is_root() && v.root() == root),
            "local plan source matches original root",
            p,
        )?;
        let factory = anchors
            .function(site.factory.target)
            .ok_or_else(|| failure("local original factory", site))?;
        let factory_symbol = one(
            names
                .symbols()
                .iter()
                .filter(|s| s.span() == factory.name_span && s.kind() == SymbolKind::Function),
            "local original factory symbol",
        )?
        .id();
        need(
            typed.call(site.local.initializer).is_some_and(|c| {
                c.target() == CallableTarget::Source(factory_symbol) && c.arguments().is_empty()
            }),
            "local exact selected initializer factory",
            site,
        )?;
        let previous = anchors
            .sites
            .iter()
            .filter(|s| s.scanner == site.scanner && s.span.start() < site.span.start())
            .collect::<Vec<_>>();
        need(
            previous.len() <= 1,
            "local fixed scanner has at most one predecessor site",
            &previous,
        )?;
        let path = previous
            .first()
            .and_then(|s| s.size.as_ref())
            .map(|s| s.condition);
        let roots = p
            .closure_phis()
            .iter()
            .filter(|phi| phi.boundary() == B::Header)
            .map(|phi| phi.symbol())
            .collect::<Vec<_>>();
        need(
            roots.len() == 1 + previous.len()
                && roots.contains(&root)
                && p.closure_phis().len() == roots.len() * 2
                && p.closure_phi_incomings().len() == 3,
            "local complete phi and incoming cardinalities",
            p,
        )?;
        for symbol in roots {
            let current = symbol == root;
            let origin = if current {
                site.local.marker
            } else {
                previous[0].local.marker
            };
            let h = one(
                p.closure_phis()
                    .iter()
                    .filter(|phi| phi.symbol() == symbol && phi.boundary() == B::Header),
                "local unique Header phi",
            )?;
            let x = one(
                p.closure_phis()
                    .iter()
                    .filter(|phi| phi.symbol() == symbol && phi.boundary() == B::Exit),
                "local unique Exit phi",
            )?;
            if !current {
                let original = one(
                    names
                        .symbols()
                        .iter()
                        .filter(|s| s.span() == origin && s.kind() == SymbolKind::Variable),
                    "local original previous binding",
                )?;
                need(
                    original.id() == symbol,
                    "local dead phi is actual preceding local",
                    (original, h),
                )?;
            }
            for phi in [h, x] {
                let selector = conditions
                    .selector(phi.availability_selector())
                    .ok_or_else(|| failure("local selector", phi))?;
                need(
                    h.owner() != x.owner()
                        && phi.root_nodes().is_empty()
                        && phi.capture_layout().is_empty()
                        && phi.origins().is_empty()
                        && matches!(conditions.owner_value(phi.owner()), Some(V::IterationPhi { statement, symbol: s, boundary, origin: o }) if *statement == site.statement && *s == symbol && *boundary == phi.boundary() && *o == site.span)
                        && matches!(selector.source(), CleanupSelectorSource::IterationPhi { statement, boundary, .. } if statement == site.statement && boundary == phi.boundary())
                        && selector.selection() == CleanupSelection::IterationPresence
                        && selector.branch_count() == 2
                        && matches!(conditions.get(phi.availability_condition()), Some(C::Choice { selector, branches }) if *selector == phi.availability_selector() && matches!(branches.as_slice(), [no, yes] if conditions.get(*no) == Some(&C::Never) && conditions.get(*yes) == Some(&C::Always))),
                    "local public phi definition and complete presence Choice",
                    (phi, selector),
                )?;
            }
            for (kind, target, point) in [
                (K::Entry, h, P::AfterExpression(site.source)),
                (K::Fallthrough, h, P::AfterStatement(site.body)),
                (K::Exhaustion, x, P::LoopExit(site.statement)),
            ] {
                let edge = one(
                    p.closure_phi_incomings()
                        .iter()
                        .filter(|e| e.kind() == kind),
                    "local exact incoming kind",
                )?;
                let binding = one(
                    edge.bindings()
                        .iter()
                        .filter(|b| b.target() == target.owner()),
                    "local complete target incoming binding",
                )?;
                need(
                    edge.bindings().len() == 1 + previous.len()
                        && edge.point() == point
                        && edge.boundary() == target.boundary()
                        && condition(conditions, edge.condition(), path, None).is_ok()
                        && binding.availability_selector() == target.availability_selector()
                        && binding.capture_slots_to_clear().is_empty()
                        && binding.root_sources().is_empty()
                        && binding.selector_writes().is_empty()
                        && binding.origins().is_empty()
                        && binding.presence_source()
                            == IterationPhiPresenceSource::StaticConditions,
                    "local incoming point, complete write shape and selector identity",
                    edge,
                )?;
                // Dead predecessor Entry is explicitly Never; subsequent iterations retain that
                // same false Header presence. It must not become an executable SSA owner column.
                if !current && kind != K::Fallthrough || kind == K::Exhaustion && !input.later {
                    need(
                        binding.values().is_empty()
                            && conditions.get(binding.available_when()) == Some(&C::Never),
                        "local absent owner has empty values and Never availability",
                        binding,
                    )?;
                } else {
                    let value = one(binding.values().iter(), "local unique incoming owner value")?;
                    need(
                        value.condition() == binding.available_when()
                            && if kind == K::Entry {
                                condition(conditions, binding.available_when(), path, None).is_ok()
                                    && matches!(conditions.owner_value(value.source()), Some(V::Expression { expression, origin }) if *expression == site.local.initializer && *origin == site.factory.span)
                            } else {
                                value.source() == h.owner()
                                    && condition(
                                        conditions,
                                        binding.available_when(),
                                        if kind == K::Exhaustion { None } else { path },
                                        Some(h.availability_selector()),
                                    )
                                    .is_ok()
                            },
                        "local exact factory birth/Header recurrence/Exit provenance",
                        binding,
                    )?;
                }
            }
        }
        let h = one(
            p.closure_phis()
                .iter()
                .filter(|phi| phi.symbol() == root && phi.boundary() == B::Header),
            "local source Header",
        )?;
        let x = one(
            p.closure_phis()
                .iter()
                .filter(|phi| phi.symbol() == root && phi.boundary() == B::Exit),
            "local source Exit",
        )?;
        let drop = one(
            owned.drops().iter().filter(|d| {
                d.target() == DropTarget::Named(root)
                    && d.point()
                        == site
                            .size
                            .as_ref()
                            .map_or(P::LoopExit(site.statement), |s| P::CallReturn(s.expression))
            }),
            "local exact cleanup obligation at actual last use",
        )?;
        let owner = if input.later { x } else { h };
        need(
            drop.owner() == Some(owner.owner())
                && drop.condition().is_some_and(|c| {
                    condition(conditions, c, path, Some(owner.availability_selector())).is_ok()
                })
                && drop.value_origin() == site.local.marker
                && drop.capture_slot().is_none()
                && drop.instance_address().is_none(),
            "local last-use Drop current owner/condition/origin",
            drop,
        )?;
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == E::Exhaustion),
            "local normal exit",
        )?;
        let mut expected = vec![
            A::FinishProvider(site.statement),
            A::EndSource(site.statement),
        ];
        if !input.later {
            expected.push(A::Drop(*drop));
        }
        if let Some(previous) = previous.first() {
            let old_root = one(
                names.symbols().iter().filter(|s| {
                    s.span() == previous.local.marker && s.kind() == SymbolKind::Variable
                }),
                "local original dead predecessor symbol",
            )?
            .id();
            let old_header = one(
                p.closure_phis()
                    .iter()
                    .filter(|phi| phi.symbol() == old_root && phi.boundary() == B::Header),
                "local dead predecessor Header",
            )?;
            let old_drop = one(
                owned.drops().iter().filter(|d| {
                    d.target() == DropTarget::Named(old_root)
                        && d.point() == P::LoopExit(site.statement)
                }),
                "local unique dead predecessor conditional drop",
            )?;
            need(
                old_drop.owner() == Some(old_header.owner())
                    && old_drop.value_origin() == previous.local.marker
                    && old_drop.capture_slot().is_none()
                    && old_drop.instance_address().is_none()
                    && old_drop.condition().is_some_and(|c| {
                        condition(
                            conditions,
                            c,
                            path,
                            Some(old_header.availability_selector()),
                        )
                        .is_ok()
                    }),
                "local dead predecessor Drop exactly guarded by proven false Header presence",
                old_drop,
            )?;
            expected.push(A::Drop(*old_drop));
        }
        need(
            exit.actions() == expected,
            "local complete normal-exit Drop inventory and source-before-owner order",
            (exit, &expected),
        )?;
    }
    Ok(())
}

// Evaluate the entire public Choice DAG under every combination of the original
// preceding If success and this phi presence. No unrelated selector is accepted.
fn condition(
    table: &CleanupConditions,
    id: CleanupConditionId,
    path: Option<ExpressionId>,
    presence: Option<CleanupSelectorId>,
) -> Result<(), Failure> {
    for success in [false, true] {
        for present in [false, true] {
            let actual = evaluate(
                table,
                id,
                path,
                presence,
                success,
                present,
                &mut std::collections::BTreeSet::new(),
            )?;
            need(
                actual == ((path.is_none() || success) && (presence.is_none() || present)),
                "local exact path and presence conjunction",
                (id, path, presence, success, present, actual),
            )?;
        }
    }
    Ok(())
}

fn evaluate(
    table: &CleanupConditions,
    id: CleanupConditionId,
    path: Option<ExpressionId>,
    presence: Option<CleanupSelectorId>,
    success: bool,
    present: bool,
    seen: &mut std::collections::BTreeSet<CleanupConditionId>,
) -> Result<bool, Failure> {
    need(seen.insert(id), "local acyclic public condition", id)?;
    match table.get(id) {
        Some(C::Always) => Ok(true),
        Some(C::Never) => Ok(false),
        Some(C::Choice { selector, branches }) => {
            let s = table
                .selector(*selector)
                .ok_or_else(|| failure("local original condition selector", selector))?;
            let branch = if Some(*selector) == presence
                && s.selection() == CleanupSelection::IterationPresence
            {
                usize::from(present)
            } else if matches!(s.source(), CleanupSelectorSource::Control(c) if Some(c) == path)
                && s.selection() == CleanupSelection::Branch
            {
                usize::from(!success)
            } else {
                return Err(failure("local condition has unrelated selector", s));
            };
            need(
                s.branch_count() == 2 && branches.len() == 2,
                "local complete condition branches",
                branches,
            )?;
            evaluate(
                table,
                branches[branch],
                path,
                presence,
                success,
                present,
                seen,
            )
        }
        None => Err(failure("local real condition exists", id)),
    }
}
