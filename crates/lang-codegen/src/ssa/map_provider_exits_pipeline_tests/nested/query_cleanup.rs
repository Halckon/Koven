//! 独立核完整 normal actions 与 caller owner；unit 只消费自己的公开事实。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::Site,
};
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

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
