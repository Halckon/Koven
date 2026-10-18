//! 独立核每一实际出口和全表同点动作；Return 共享完整序列只核一次。
#[path = "query_phi.rs"]
mod query_phi;
#[path = "query_cleanup_unit.rs"]
mod unit_cleanup;

use super::super::ast::{Anchors, Exit, Input, Size};
use super::{Failure, failure, need, one};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupOwnerValue, CleanupOwnerValueId, DropFact,
        DropPoint, DropTarget, IterationCleanupAction as A, IterationExitKind as E,
        OwnershipCheckedFile,
    },
    source::Span,
};
pub(super) use unit_cleanup::{caller as unit_caller, check as unit};

pub(super) fn single(
    f: &Input,
    a: &Anchors,
    guards: [SymbolId; 2],
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let current = query_phi::check(f, a, guards[0], owned)?;
    let transfer = DropPoint::ControlTransfer(a.transfer);
    let after_outer = DropPoint::AfterStatement(a.sites[0].body);
    let inner_drop = actual_drop(owned, guards[1], transfer)?;
    let inner_owner = inner_drop
        .owner()
        .ok_or_else(|| failure("nested exits FE GI construction owner", inner_drop))?;
    need(
        matches!(owned.cleanup_conditions().owner_value(inner_owner),
        Some(CleanupOwnerValue::Expression { expression, origin })
        if *expression == a.guards[1].initializer && *origin == a.guards[1].span)
            && ![current.initial, current.header, current.exit].contains(&inner_owner),
        "nested exits FE GI owner is separate real construction",
        (inner_drop, current),
    )?;
    check_drop(inner_drop, a.guards[1].marker, inner_owner, None)?;
    let outer_drop = actual_drop(owned, guards[0], after_outer)?;
    check_drop(
        outer_drop,
        a.guards[0].marker,
        current.exit,
        Some(current.exit_condition),
    )?;
    let mut expected_drops = vec![inner_drop, outer_drop];
    let outer = owned
        .map_iteration(a.sites[0].statement)
        .ok_or_else(|| failure("nested exits FE outer cleanup plan", &a.sites[0]))?;
    let inner = owned
        .map_iteration(a.sites[1].statement)
        .ok_or_else(|| failure("nested exits FE inner cleanup plan", &a.sites[1]))?;
    let finish = |index: usize| {
        vec![
            A::FinishProvider(a.sites[index].statement),
            A::EndSource(a.sites[index].statement),
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
                statement: a.sites[index].statement,
                symbol: d.value_binding(),
            },
            A::EndBinding {
                statement: a.sites[index].statement,
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
        let outer_return = actual_drop(owned, guards[0], transfer)?;
        check_drop(
            outer_return,
            a.guards[0].marker,
            current.header,
            Some(current.header_condition),
        )?;
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
        "nested exits FE exact full transfer action count",
        &exit_actions,
    )?;
    let mut outer_actions = vec![A::Drop(*outer_drop)];
    outer_actions.extend(end_bindings(0));
    let points = [
        (transfer, exit_actions),
        (DropPoint::LoopExit(a.sites[1].statement), finish(1)),
        (after_outer, outer_actions),
        (DropPoint::LoopExit(a.sites[0].statement), finish(0)),
    ];
    let exit_kind = match f.exit {
        Exit::Continue => E::Continue(a.transfer),
        Exit::Break => E::Break(a.transfer),
        Exit::Return => E::Return(a.transfer),
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
            "nested exits FE exact per-provider exit coverage",
            plan.exits(),
        )?;
        for (kind, point_index) in expected {
            let actual = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "nested exits FE unique exit by actual transfer identity",
            )?;
            let (point, actions) = &points[point_index];
            need(
                actual.point() == *point
                    && actual.actions() == actions
                    && owned.cleanup_conditions().get(actual.condition())
                        == Some(&CleanupCondition::Always),
                "nested exits FE full ordered per-plan action sequence and Always",
                (actual, actions),
            )?;
        }
        let descriptor = plan.descriptor();
        need(
            !owned.drops().iter().any(|drop| {
                drop.target() == DropTarget::Temporary(a.sites[index].source)
                    || descriptor
                        .bindings()
                        .iter()
                        .any(|id| drop.target() == DropTarget::Named(*id))
            }),
            "nested exits FE borrowed source and components have no owner Drop",
            plan,
        )?;
    }
    // 完整 cleanup_steps 按 point 过滤；共享 Return 的双 plan 不再把同点动作拼接两遍。
    for (index, (point, actions)) in points.iter().enumerate() {
        need(
            !points[..index].iter().any(|(prior, _)| prior == point),
            "nested exits FE unique whole cleanup point",
            point,
        )?;
        let complete = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == *point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            complete == *actions,
            "nested exits FE complete all-table same-point ordered cleanup",
            (point, complete, actions),
        )?;
    }
    let owners = [current.initial, current.header, current.exit, inner_owner];
    let related = |drop: &DropFact| {
        guards
            .iter()
            .any(|id| drop.target() == DropTarget::Named(*id))
            || drop.owner().is_some_and(|id| owners.contains(&id))
    };
    let all_drops = owned
        .drops()
        .iter()
        .filter(|d| related(d))
        .collect::<Vec<_>>();
    let all_steps = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            A::Drop(drop) if related(drop) => Some(drop),
            _ => None,
        })
        .collect::<Vec<_>>();
    for all in [&all_drops, &all_steps] {
        need(
            all.len() == expected_drops.len()
                && expected_drops
                    .iter()
                    .all(|drop| all.iter().filter(|v| *v == drop).count() == 1),
            "nested exits FE all related GO/GI owner/target Drop uses exactly accounted",
            (all, &expected_drops),
        )?;
    }
    Ok(())
}

fn actual_drop(
    owned: &OwnershipCheckedFile,
    local: SymbolId,
    point: DropPoint,
) -> Result<&DropFact, Failure> {
    one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(local) && d.point() == point),
        "nested exits FE unique actual Guard Drop target and point",
    )
}

fn check_drop(
    drop: &DropFact,
    origin: Span,
    owner: CleanupOwnerValueId,
    condition: Option<CleanupConditionId>,
) -> Result<(), Failure> {
    need(
        drop.owner() == Some(owner)
            && drop.value_origin() == origin
            && drop.condition() == condition
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none(),
        "nested exits FE exact current Guard owner, condition, origin and no slot/address",
        drop,
    )
}

pub(super) fn single_caller(
    f: &Input,
    local: SymbolId,
    marker: Span,
    initializer: ExpressionId,
    size: &Size,
    owned: &OwnershipCheckedFile,
) -> Result<(Span, CleanupOwnerValueId), Failure> {
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(local)),
        "nested exits FE unique actual caller local Drop",
    )?;
    let owner = drop
        .owner()
        .ok_or_else(|| failure("nested exits FE caller Drop owner", drop))?;
    let value = owned.cleanup_conditions().owner_value(owner);
    let initializer_span = f.expression(initializer)?.1;
    need(
        drop.point() == DropPoint::CallReturn(size.expression)
            && drop.value_origin() == marker
            && drop.condition().is_none()
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none()
            && matches!(value, Some(CleanupOwnerValue::Expression { expression, origin })
            if *expression == initializer && *origin == initializer_span),
        "nested exits FE caller owner is initializer and Drop follows actual size",
        (marker, initializer, size, drop, value),
    )?;
    need(
        owned
            .drops()
            .iter()
            .filter(|d| d.owner() == Some(owner))
            .count()
            == 1,
        "nested exits FE caller initializer owner has one exact Drop use",
        owner,
    )?;
    Ok((drop.value_origin(), owner))
}
