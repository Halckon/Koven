//! Unit 只核公开 qualified drops/plans/同表 getter，不制造 single owner/phi 表。
use super::super::super::ast::{Anchors, Exit, Input};
use super::{Failure, failure, need, one};
use lang_frontend::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropFact, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as A, UnitIterationExitKind as E,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};
pub(in super::super) fn check(
    f: &Input,
    a: &Anchors,
    guards: [UnitSymbolId; 2],
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
) -> Result<[Span; 2], Failure> {
    let expression = |id| UnitExpressionId::new(unit, id);
    let mut expected_drops = Vec::new();
    let mut points = Vec::new();
    let mut origins = Vec::new();
    for (index, site) in a.sites.iter().enumerate() {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        let transfer_id = expression(site.transfer);
        let plan = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary exits FE actual qualified cleanup plan", site))?;
        let d = plan.descriptor();
        let transfer = UnitDropPoint::ControlTransfer(transfer_id);
        let exhausted = UnitDropPoint::LoopExit(statement);
        let guard = actual_drop(owned, UnitDropTarget::Named(guards[index]), transfer)?;
        let source_drop = actual_drop(owned, UnitDropTarget::Temporary(source), exhausted)?;
        need(
            guards[index].source_unit() == unit
                && guard.value_origin() == a.guards[index].marker
                && source_drop.value_origin() == site.source_span,
            "temporary exits FE actual qualified Guard marker and factory Call drop origins",
            (guard, source_drop),
        )?;
        expected_drops.extend([*guard, *source_drop]);
        origins.push(source_drop.value_origin());
        let mut actions = vec![
            A::Drop(*guard),
            A::EndBinding {
                statement,
                symbol: d.value_binding(),
            },
            A::EndBinding {
                statement,
                symbol: d.key_binding(),
            },
        ];
        if f.exit != Exit::Continue {
            let moved = actual_drop(owned, UnitDropTarget::Temporary(source), transfer)?;
            need(
                moved.value_origin() == site.source_span,
                "temporary exits FE qualified transfer Drop actual source Call origin",
                moved,
            )?;
            actions.extend([
                A::FinishProvider(statement),
                A::EndSource(statement),
                A::Drop(*moved),
            ]);
            expected_drops.push(*moved);
        }
        need(
            actions.len() == if f.exit == Exit::Continue { 3 } else { 6 },
            "temporary exits FE qualified exact transfer action count",
            &actions,
        )?;
        let false_actions = vec![
            A::FinishProvider(statement),
            A::EndSource(statement),
            A::Drop(*source_drop),
        ];
        let kind = match f.exit {
            Exit::Continue => E::Continue(transfer_id),
            Exit::Break => E::Break(transfer_id),
            Exit::Return => E::Return(transfer_id),
        };
        need(
            plan.exits().len() == 2,
            "temporary exits FE qualified exact transfer and false-terminal exits",
            plan.exits(),
        )?;
        for (kind, point, actions) in [
            (kind, transfer, actions),
            (E::Exhaustion, exhausted, false_actions),
        ] {
            let exit = one(
                plan.exits().iter().filter(|exit| exit.kind() == kind),
                "temporary exits FE unique qualified actual exit kind",
            )?;
            need(
                exit.point() == point && exit.actions() == actions,
                "temporary exits FE independently expected qualified complete actions",
                (exit, &actions),
            )?;
            // 该 accessor 只筛同表，不能声称独立证明 acquired/owner/loan-end。
            need(
                owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "temporary exits FE qualified same-table full-point getter consistency",
                (point, &actions),
            )?;
            let plans = owned
                .map_iterations()
                .iter()
                .filter(|p| p.exits().iter().any(|e| e.point() == point))
                .collect::<Vec<_>>();
            need(
                plans.len() == 1 && plans[0].descriptor().statement() == statement,
                "temporary exits FE qualified point belongs to one independent provider",
                (point, plans),
            )?;
            points.push(point);
        }
    }
    need(
        points.len() == 4
            && points
                .iter()
                .enumerate()
                .all(|(i, p)| !points[..i].contains(p)),
        "temporary exits FE four distinct qualified cleanup points",
        &points,
    )?;
    let related = |drop: &UnitDropFact| {
        guards
            .iter()
            .any(|id| drop.target() == UnitDropTarget::Named(*id))
            || a.sites
                .iter()
                .any(|site| drop.target() == UnitDropTarget::Temporary(expression(site.source)))
    };
    let actual = owned
        .drops()
        .iter()
        .filter(|d| related(d))
        .copied()
        .collect::<Vec<_>>();
    need(
        actual.len() == expected_drops.len()
            && expected_drops
                .iter()
                .all(|d| actual.iter().filter(|actual| *actual == d).count() == 1),
        "temporary exits FE complete qualified relevant-target Drop coverage",
        (actual, expected_drops),
    )?;
    origins
        .try_into()
        .map_err(|v| failure("temporary exits FE two qualified source Drop spans", v))
}
fn actual_drop(
    owned: &CompilationUnitOwnership,
    target: UnitDropTarget,
    point: UnitDropPoint,
) -> Result<&UnitDropFact, Failure> {
    one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == target && d.point() == point),
        "temporary exits FE unique qualified actual target at point",
    )
}
