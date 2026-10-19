//! 两 site 的真实 temporary owner 与 normal 清理；unit 不借用 single 的 owner API。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::Site,
};
use lang_frontend::{
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupOwnerValueId, CompilationUnitOwnership,
        DropPoint, DropTarget, IterationCleanupAction as A, IterationExitKind as E,
        IterationPhiBoundary as B, IterationPhiIncomingKind as K, OwnershipCheckedFile,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

pub(super) fn single(
    site: &Site,
    owned: &OwnershipCheckedFile,
) -> Result<(Span, CleanupOwnerValueId), Failure> {
    let plan = owned
        .map_iteration(site.statement)
        .ok_or_else(|| failure("temporary FE actual cleanup plan", site))?;
    let descriptor = plan.descriptor();
    let drops = owned
        .drops()
        .iter()
        .filter(|d| d.target() == DropTarget::Temporary(site.source));
    let drop = one(drops, "temporary FE unique actual source Drop")?;
    let owner = drop
        .owner()
        .ok_or_else(|| failure("temporary FE source Drop owner", drop))?;
    let table = owned.cleanup_conditions();
    need(
        drop.point() == DropPoint::LoopExit(site.statement)
            && drop.value_origin() == site.source_span
            && drop.condition().is_none()
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none()
            && matches!(table.owner_value(owner), Some(CleanupOwnerValue::Expression { expression, origin })
                if *expression == site.source && *origin == site.source_span),
        "temporary FE source owner is actual factory Call expression",
        (site, drop, table.owner_value(owner)),
    )?;
    let expected = [
        (
            E::Fallthrough,
            DropPoint::AfterStatement(site.body),
            vec![
                A::EndBinding {
                    statement: site.statement,
                    symbol: descriptor.value_binding(),
                },
                A::EndBinding {
                    statement: site.statement,
                    symbol: descriptor.key_binding(),
                },
            ],
        ),
        (
            E::Exhaustion,
            DropPoint::LoopExit(site.statement),
            vec![
                A::FinishProvider(site.statement),
                A::EndSource(site.statement),
                A::Drop(*drop),
            ],
        ),
    ];
    need(
        plan.exits().len() == expected.len(),
        "temporary FE exact two normal exits",
        plan.exits(),
    )?;
    for (kind, point, actions) in expected {
        let exit = one(
            plan.exits().iter().filter(|e| e.kind() == kind),
            "temporary FE unique normal exit",
        )?;
        need(
            exit.point() == point
                && exit.actions() == actions
                && table.get(exit.condition()) == Some(&CleanupCondition::Always),
            "temporary FE independent ordered normal actions and Always",
            (exit, &actions),
        )?;
        let complete = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            complete == actions,
            "temporary FE same-point accessor consistency",
            (point, complete, actions),
        )?;
    }
    need(
        plan.closure_phis().is_empty() && plan.closure_phi_incomings().len() == 3,
        "temporary FE no public owner phi and exactly three incoming",
        plan,
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
        let incoming = one(
            plan.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "temporary FE unique incoming by actual kind",
        )?;
        need(
            incoming.point() == point
                && incoming.boundary() == boundary
                && incoming.bindings().is_empty()
                && table.get(incoming.condition()) == Some(&CleanupCondition::Always),
            "temporary FE actual incoming point/boundary/Always/empty bindings",
            incoming,
        )?;
    }
    Ok((drop.value_origin(), owner))
}

pub(super) fn unit(
    site: &Site,
    statement: UnitStatementId,
    source: UnitExpressionId,
    owned: &CompilationUnitOwnership,
) -> Result<Span, Failure> {
    let plan = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("temporary FE actual qualified cleanup plan", statement))?;
    let descriptor = plan.descriptor();
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Temporary(source)),
        "temporary FE unique qualified source Drop",
    )?;
    need(
        drop.point() == UnitDropPoint::LoopExit(statement)
            && drop.value_origin() == site.source_span,
        "temporary FE actual qualified Call Drop point and origin",
        (site, drop),
    )?;
    let expected = [
        (
            UE::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), site.body)),
            vec![
                UA::EndBinding {
                    statement,
                    symbol: descriptor.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: descriptor.key_binding(),
                },
            ],
        ),
        (
            UE::Exhaustion,
            UnitDropPoint::LoopExit(statement),
            vec![
                UA::FinishProvider(statement),
                UA::EndSource(statement),
                UA::Drop(*drop),
            ],
        ),
    ];
    need(
        plan.exits().len() == expected.len(),
        "temporary FE exact qualified two normal exits",
        plan.exits(),
    )?;
    for (kind, point, actions) in expected {
        let exit = one(
            plan.exits().iter().filter(|e| e.kind() == kind),
            "temporary FE unique qualified exit",
        )?;
        need(
            exit.point() == point && exit.actions() == actions,
            "temporary FE independently expected qualified cleanup",
            (exit, &actions),
        )?;
        need(
            owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "temporary FE qualified same-table accessor consistency",
            (point, &actions),
        )?;
    }
    Ok(drop.value_origin())
}
