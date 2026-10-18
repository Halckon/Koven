//! 独立核完整 normal actions 与 caller owner；unit 只消费自己的公开事实。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Input, Site, Size},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SymbolId, UnitSymbolId},
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

pub(super) fn single_site(site: &Site, owned: &OwnershipCheckedFile) -> Result<(), Failure> {
    let plan = owned
        .map_iteration(site.statement)
        .ok_or_else(|| failure("nested FE actual cleanup plan", site))?;
    let descriptor = plan.descriptor();
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
            ],
        ),
    ];
    need(
        plan.exits().len() == expected.len(),
        "nested FE exact two normal exits",
        plan.exits(),
    )?;
    let table = owned.cleanup_conditions();
    for (kind, point, actions) in expected {
        let exit = one(
            plan.exits().iter().filter(|e| e.kind() == kind),
            "nested FE unique normal exit",
        )?;
        need(
            exit.point() == point
                && exit.actions() == actions
                && table.get(exit.condition()) == Some(&CleanupCondition::Always),
            "nested FE independent full ordered actions and Always",
            (exit, &actions),
        )?;
        let complete = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            complete == actions,
            "nested FE complete same-point cleanup consistency",
            (point, complete, actions),
        )?;
    }
    need(
        plan.closure_phis().is_empty() && plan.closure_phi_incomings().len() == 3,
        "nested FE no owner phi and exactly three incoming facts",
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
            "nested FE unique incoming by actual kind",
        )?;
        need(
            incoming.point() == point
                && incoming.boundary() == boundary
                && incoming.bindings().is_empty()
                && table.get(incoming.condition()) == Some(&CleanupCondition::Always),
            "nested FE incoming exact point boundary Always and no bindings",
            incoming,
        )?;
    }
    need(
        !owned
            .drops()
            .iter()
            .any(|d| d.target() == DropTarget::Temporary(site.source)),
        "nested FE borrowed provider source has no temporary Drop",
        site,
    )
}

pub(super) fn unit_site(
    site: &Site,
    statement: UnitStatementId,
    source: UnitExpressionId,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let plan = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("nested FE qualified cleanup plan", statement))?;
    let descriptor = plan.descriptor();
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
            vec![UA::FinishProvider(statement), UA::EndSource(statement)],
        ),
    ];
    need(
        plan.exits().len() == expected.len(),
        "nested FE exact two qualified normal exits",
        plan.exits(),
    )?;
    for (kind, point, actions) in expected {
        let exit = one(
            plan.exits().iter().filter(|e| e.kind() == kind),
            "nested FE unique qualified normal exit",
        )?;
        need(
            exit.point() == point && exit.actions() == actions,
            "nested FE qualified full ordered actions",
            (exit, &actions),
        )?;
        // 这是同表一致性；不假造 unit 未公开的 condition、owner 或 phi 模型。
        need(
            owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "nested FE qualified complete same-point cleanup consistency",
            (point, &actions),
        )?;
    }
    need(
        !owned
            .drops()
            .iter()
            .any(|d| d.target() == UnitDropTarget::Temporary(source)),
        "nested FE qualified borrowed source has no temporary Drop",
        source,
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
        "nested FE unique actual caller local Drop",
    )?;
    let owner = drop
        .owner()
        .ok_or_else(|| failure("nested FE caller Drop owner", drop))?;
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
        "nested FE caller owner is initializer and Drop follows actual size",
        (marker, initializer, size, drop, value),
    )?;
    Ok((drop.value_origin(), owner))
}

pub(super) fn unit_caller(
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
        "nested FE unique qualified caller local Drop",
    )?;
    need(
        drop.point() == UnitDropPoint::CallReturn(size) && drop.value_origin() == marker,
        "nested FE qualified caller Drop follows actual size with local origin",
        (local, marker, size, drop),
    )?;
    Ok(drop.value_origin())
}
