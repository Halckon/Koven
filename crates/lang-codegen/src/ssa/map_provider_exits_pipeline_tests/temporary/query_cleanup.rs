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
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

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
