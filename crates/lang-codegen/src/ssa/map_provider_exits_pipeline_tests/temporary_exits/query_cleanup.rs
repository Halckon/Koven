//! 两个独立 source 的四个实际清理点；expected 独立于 plan/actions 构造。
#[path = "query_cleanup_unit.rs"]
mod unit_cleanup;
use super::super::ast::{Anchors, Exit, Input};
use super::{Failure, failure, need, one};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupOwnerValueId, DropFact, DropPoint, DropTarget,
        IterationCleanupAction as A, IterationExitKind as E, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, OwnershipCheckedFile,
    },
    source::Span,
};
pub(super) use unit_cleanup::check as unit;

pub(super) fn single(
    f: &Input,
    a: &Anchors,
    guards: [SymbolId; 2],
    owned: &OwnershipCheckedFile,
) -> Result<[Span; 2], Failure> {
    let mut expected_drops = Vec::new();
    let mut owners = Vec::new();
    let mut points = Vec::new();
    let mut origins = Vec::new();
    for (index, site) in a.sites.iter().enumerate() {
        let plan = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("temporary exits FE actual cleanup plan", site))?;
        let d = plan.descriptor();
        let transfer = DropPoint::ControlTransfer(site.transfer);
        let exhausted = DropPoint::LoopExit(site.statement);
        let guard = actual_drop(owned, DropTarget::Named(guards[index]), transfer)?;
        let guard_owner = expression_owner(
            owned,
            guard,
            a.guards[index].initializer,
            a.guards[index].span,
            a.guards[index].marker,
        )?;
        let source_drop = actual_drop(owned, DropTarget::Temporary(site.source), exhausted)?;
        let source_owner = expression_owner(
            owned,
            source_drop,
            site.source,
            site.source_span,
            site.source_span,
        )?;
        need(
            guard_owner != source_owner,
            "temporary exits FE construction and factory own separate actual Expression owners",
            (guard_owner, source_owner),
        )?;
        expected_drops.extend([*guard, *source_drop]);
        owners.extend([guard_owner, source_owner]);
        origins.push(source_drop.value_origin());
        let mut actions = vec![
            A::Drop(*guard),
            A::EndBinding {
                statement: site.statement,
                symbol: d.value_binding(),
            },
            A::EndBinding {
                statement: site.statement,
                symbol: d.key_binding(),
            },
        ];
        if f.exit != Exit::Continue {
            let moved = actual_drop(owned, DropTarget::Temporary(site.source), transfer)?;
            let owner = expression_owner(
                owned,
                moved,
                site.source,
                site.source_span,
                site.source_span,
            )?;
            need(
                owner == source_owner,
                "temporary exits FE transfer and exhaustion retain same factory Expression owner",
                (moved, source_drop),
            )?;
            actions.extend([
                A::FinishProvider(site.statement),
                A::EndSource(site.statement),
                A::Drop(*moved),
            ]);
            expected_drops.push(*moved);
        }
        need(
            actions.len() == if f.exit == Exit::Continue { 3 } else { 6 },
            "temporary exits FE exact independent transfer action count",
            &actions,
        )?;
        let false_actions = vec![
            A::FinishProvider(site.statement),
            A::EndSource(site.statement),
            A::Drop(*source_drop),
        ];
        let kind = match f.exit {
            Exit::Continue => E::Continue(site.transfer),
            Exit::Break => E::Break(site.transfer),
            Exit::Return => E::Return(site.transfer),
        };
        need(
            plan.exits().len() == 2,
            "temporary exits FE exact transfer and false-terminal exits",
            plan.exits(),
        )?;
        for (kind, point, actions) in [
            (kind, transfer, actions),
            (E::Exhaustion, exhausted, false_actions),
        ] {
            let exit = one(
                plan.exits().iter().filter(|exit| exit.kind() == kind),
                "temporary exits FE unique actual exit kind",
            )?;
            need(
                exit.point() == point
                    && exit.actions() == actions
                    && owned.cleanup_conditions().get(exit.condition())
                        == Some(&CleanupCondition::Always),
                "temporary exits FE independent ordered complete actions and Always",
                (exit, &actions),
            )?;
            let complete = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(at, action)| (*at == point).then_some(*action))
                .collect::<Vec<_>>();
            let plans = owned
                .map_iterations()
                .iter()
                .filter(|p| p.exits().iter().any(|e| e.point() == point))
                .collect::<Vec<_>>();
            need(
                complete == actions
                    && plans.len() == 1
                    && plans[0].descriptor().statement() == site.statement,
                "temporary exits FE full same-point sequence belongs to only this independent provider",
                (point, complete, plans),
            )?;
            points.push(point);
        }
        need(
            plan.closure_phis().is_empty()
                && plan.closure_flow().header().is_empty()
                && plan.closure_flow().exit().is_empty()
                && plan.capture_graph().nodes().is_empty(),
            "temporary exits FE no provider closure owner phi or capture state",
            plan,
        )?;
        let mut expected = vec![(K::Entry, DropPoint::AfterExpression(site.source), B::Header)];
        match f.exit {
            Exit::Continue => expected.push((K::Continue(site.transfer), transfer, B::Header)),
            Exit::Break => expected.push((K::Break(site.transfer), transfer, B::Exit)),
            Exit::Return => {}
        }
        expected.push((K::Exhaustion, exhausted, B::Exit));
        need(
            plan.closure_phi_incomings().len() == expected.len(),
            "temporary exits FE C/B three and R two actual incoming edges",
            plan.closure_phi_incomings(),
        )?;
        for (kind, point, boundary) in expected {
            let incoming = one(
                plan.closure_phi_incomings()
                    .iter()
                    .filter(|i| i.kind() == kind),
                "temporary exits FE unique actual incoming",
            )?;
            need(
                incoming.point() == point
                    && incoming.boundary() == boundary
                    && incoming.bindings().is_empty()
                    && owned.cleanup_conditions().get(incoming.condition())
                        == Some(&CleanupCondition::Always),
                "temporary exits FE exact incoming point boundary Always empty bindings",
                incoming,
            )?;
        }
    }
    need(
        points.len() == 4
            && points
                .iter()
                .enumerate()
                .all(|(i, p)| !points[..i].contains(p))
            && owners.len() == 4
            && owners
                .iter()
                .enumerate()
                .all(|(i, v)| !owners[..i].contains(v)),
        "temporary exits FE four independent points and construction/source owners",
        (&points, &owners),
    )?;
    let related = |drop: &DropFact| {
        guards
            .iter()
            .any(|id| drop.target() == DropTarget::Named(*id))
            || a.sites
                .iter()
                .any(|site| drop.target() == DropTarget::Temporary(site.source))
            || drop.owner().is_some_and(|owner| owners.contains(&owner))
    };
    let actual = owned
        .drops()
        .iter()
        .filter(|d| related(d))
        .copied()
        .collect::<Vec<_>>();
    let steps = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            A::Drop(drop) if related(drop) => Some(*drop),
            _ => None,
        })
        .collect::<Vec<_>>();
    need(
        actual.len() == expected_drops.len()
            && expected_drops
                .iter()
                .all(|d| actual.iter().filter(|actual| *actual == d).count() == 1)
            && steps.len() == expected_drops.len()
            && expected_drops
                .iter()
                .all(|d| steps.iter().filter(|step| *step == d).count() == 1),
        "temporary exits FE relevant targets and ANY-target owner Drops closed over actual exits",
        (actual, steps, expected_drops),
    )?;
    origins
        .try_into()
        .map_err(|v| failure("temporary exits FE two actual source Drop spans", v))
}
fn actual_drop(
    owned: &OwnershipCheckedFile,
    target: DropTarget,
    point: DropPoint,
) -> Result<&DropFact, Failure> {
    one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == target && d.point() == point),
        "temporary exits FE unique actual target at cleanup point",
    )
}
fn expression_owner(
    owned: &OwnershipCheckedFile,
    drop: &DropFact,
    expression: ExpressionId,
    origin: Span,
    diagnostic: Span,
) -> Result<CleanupOwnerValueId, Failure> {
    let owner = drop
        .owner()
        .ok_or_else(|| failure("temporary exits FE actual Drop owner", drop))?;
    need(
        drop.value_origin() == diagnostic
            && drop.condition().is_none()
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none()
            && matches!(owned.cleanup_conditions().owner_value(owner),
            Some(CleanupOwnerValue::Expression { expression: actual, origin: span }) if *actual == expression && *span == origin),
        "temporary exits FE marker only diagnostic, ownership is exact actual Expression",
        (drop, owned.cleanup_conditions().owner_value(owner)),
    )?;
    Ok(owner)
}
