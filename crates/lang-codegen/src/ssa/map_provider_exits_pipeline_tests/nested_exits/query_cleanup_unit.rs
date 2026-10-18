//! Unit 仅检查它自己的公开退出/Drop 表，getter 明确只是同表一致性。
use super::super::super::ast::{Anchors, Exit, Input};
use super::{Failure, failure, need, one};
use lang_frontend::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropFact, UnitDropPoint as P, UnitDropTarget,
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
) -> Result<(), Failure> {
    let expression = |id| UnitExpressionId::new(unit, id);
    let statement = |id| UnitStatementId::new(unit, id);
    let transfer_id = expression(a.transfer);
    let transfer = P::ControlTransfer(transfer_id);
    let after_outer = P::AfterStatement(statement(a.sites[0].body));
    let inner_drop = actual_drop(owned, guards[1], transfer, a.guards[1].marker)?;
    let outer_drop = actual_drop(owned, guards[0], after_outer, a.guards[0].marker)?;
    let mut expected_drops = vec![inner_drop, outer_drop];
    let outer = owned
        .map_iteration(statement(a.sites[0].statement))
        .ok_or_else(|| failure("nested exits FE qualified outer cleanup plan", &a.sites[0]))?;
    let inner = owned
        .map_iteration(statement(a.sites[1].statement))
        .ok_or_else(|| failure("nested exits FE qualified inner cleanup plan", &a.sites[1]))?;
    let finish = |index: usize| {
        vec![
            A::FinishProvider(statement(a.sites[index].statement)),
            A::EndSource(statement(a.sites[index].statement)),
        ]
    };
    let end_bindings = |index: usize| {
        let d = if index == 0 {
            outer.descriptor()
        } else {
            inner.descriptor()
        };
        vec![
            A::EndBinding {
                statement: statement(a.sites[index].statement),
                symbol: d.value_binding(),
            },
            A::EndBinding {
                statement: statement(a.sites[index].statement),
                symbol: d.key_binding(),
            },
        ]
    };
    let mut exit_actions = vec![A::Drop(*inner_drop)];
    exit_actions.extend(end_bindings(1));
    if f.exit != Exit::Continue {
        exit_actions.extend(finish(1));
    }
    if f.exit == Exit::Return {
        let outer_return = actual_drop(owned, guards[0], transfer, a.guards[0].marker)?;
        expected_drops.push(outer_return);
        exit_actions.push(A::Drop(*outer_return));
        exit_actions.extend(end_bindings(0));
        exit_actions.extend(finish(0));
    }
    need(
        exit_actions.len()
            == match f.exit {
                Exit::Continue => 3,
                Exit::Break => 5,
                Exit::Return => 10,
            },
        "nested exits FE qualified full transfer action count",
        &exit_actions,
    )?;
    let mut outer_actions = vec![A::Drop(*outer_drop)];
    outer_actions.extend(end_bindings(0));
    let points = [
        (transfer, exit_actions),
        (P::LoopExit(statement(a.sites[1].statement)), finish(1)),
        (after_outer, outer_actions),
        (P::LoopExit(statement(a.sites[0].statement)), finish(0)),
    ];
    let exit_kind = match f.exit {
        Exit::Continue => E::Continue(transfer_id),
        Exit::Break => E::Break(transfer_id),
        Exit::Return => E::Return(transfer_id),
    };
    for (index, plan) in [outer, inner].into_iter().enumerate() {
        let mut expected = if index == 1 {
            vec![(exit_kind, 0), (E::Exhaustion, 1)]
        } else {
            vec![(E::Fallthrough, 2), (E::Exhaustion, 3)]
        };
        if index == 0 && f.exit == Exit::Return {
            expected.push((exit_kind, 0));
        }
        need(
            plan.exits().len() == expected.len(),
            "nested exits FE qualified exact per-provider exit coverage",
            plan.exits(),
        )?;
        for (kind, point_index) in expected {
            let actual = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "nested exits FE unique qualified actual transfer exit",
            )?;
            let (point, actions) = &points[point_index];
            need(
                actual.point() == *point && actual.actions() == actions,
                "nested exits FE qualified full ordered per-plan sequence",
                (actual, actions),
            )?;
        }
        let descriptor = plan.descriptor();
        need(
            !owned.drops().iter().any(|drop| {
                drop.target() == UnitDropTarget::Temporary(expression(a.sites[index].source))
                    || descriptor
                        .bindings()
                        .iter()
                        .any(|id| drop.target() == UnitDropTarget::Named(*id))
            }),
            "nested exits FE qualified borrowed source/components have no Drop",
            plan,
        )?;
    }
    // 两个 Return plan 上方各核同一完整十动作；此 getter 按独立实际 point 只核一次。
    // Unit 没有公开 single 的 owner/condition/phi 表；这里不借用或假造那套事实。
    for (index, (point, actions)) in points.iter().enumerate() {
        need(
            !points[..index].iter().any(|(prior, _)| prior == point),
            "nested exits FE qualified unique full cleanup point",
            point,
        )?;
        need(
            owned.iteration_cleanup_at(*point) == Some(actions.as_slice()),
            "nested exits FE qualified full same-table cleanup getter consistency",
            (point, actions),
        )?;
    }
    let actual_drops = owned
        .drops()
        .iter()
        .filter(|d| {
            guards
                .iter()
                .any(|id| d.target() == UnitDropTarget::Named(*id))
        })
        .collect::<Vec<_>>();
    need(
        actual_drops.len() == expected_drops.len()
            && expected_drops
                .iter()
                .all(|drop| actual_drops.iter().filter(|v| *v == drop).count() == 1),
        "nested exits FE qualified all Guard Drop uses exactly accounted",
        (&actual_drops, expected_drops),
    )?;
    Ok(())
}

fn actual_drop(
    owned: &CompilationUnitOwnership,
    local: UnitSymbolId,
    point: P,
    origin: Span,
) -> Result<&UnitDropFact, Failure> {
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(local) && d.point() == point),
        "nested exits FE unique actual qualified Guard Drop target and point",
    )?;
    need(
        drop.value_origin() == origin,
        "nested exits FE qualified Guard Drop actual origin",
        drop,
    )?;
    Ok(drop)
}

pub(in super::super) fn caller(
    local: UnitSymbolId,
    marker: Span,
    size: UnitExpressionId,
    owned: &CompilationUnitOwnership,
) -> Result<Span, Failure> {
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(local)),
        "nested exits FE unique qualified caller local Drop",
    )?;
    need(
        drop.point() == P::CallReturn(size) && drop.value_origin() == marker,
        "nested exits FE qualified caller Drop follows actual size with local origin",
        (local, marker, size, drop),
    )?;
    Ok(drop.value_origin())
}
