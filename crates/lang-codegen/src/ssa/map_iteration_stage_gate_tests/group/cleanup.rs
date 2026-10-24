//! 完整 normal 清理与 direct-own LoopExit owner 旁证；只读，不构造事实。
use super::super::fixture::{File, LoopSite};
use super::Report;
use lang_frontend::{
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind, UnitLoanTarget, UnitMapIterationOwnershipPlan,
    },
    parser::Statement,
    type_checking::UnitStatementId,
};

pub(super) fn unit(
    file: &File,
    site: &LoopSite,
    plan: &UnitMapIterationOwnershipPlan,
    owned: &CompilationUnitOwnership,
    is_owned: bool,
    report: &mut Report,
) {
    let statement = plan.descriptor().statement();
    let unit = statement.source_unit();
    let body = file
        .parsed
        .ast()
        .statements()
        .get(site.statement)
        .ok()
        .and_then(|node| {
            if let Statement::For { body, .. } = node.payload() {
                Some(UnitStatementId::new(unit, *body))
            } else {
                None
            }
        });
    let falls = plan
        .exits()
        .iter()
        .filter(|exit| exit.kind() == UnitIterationExitKind::Fallthrough)
        .collect::<Vec<_>>();
    let ends = plan
        .exits()
        .iter()
        .filter(|exit| exit.kind() == UnitIterationExitKind::Exhaustion)
        .collect::<Vec<_>>();
    report.check(
        "unit exactly normal body and exhaustion",
        plan.exits().len() == 2 && falls.len() == 1 && ends.len() == 1,
        plan.exits(),
    );
    let ([fall], [end]) = (falls.as_slice(), ends.as_slice()) else {
        return;
    };
    let descriptor = plan.descriptor();
    report.check(
        "unit complete normal body actions and point",
        Some(fall.point()) == body.map(UnitDropPoint::AfterStatement)
            && fall.actions()
                == [
                    UA::EndBinding {
                        statement,
                        symbol: descriptor.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: descriptor.key_binding(),
                    },
                ]
            && end.point() == UnitDropPoint::LoopExit(statement),
        (fall, end),
    );
    for exit in [fall, end] {
        // unit getter 是同表公共查询，不声称独立存储。
        report.check(
            "unit complete same-point cleanup getter",
            owned.iteration_cleanup_at(exit.point()) == Some(exit.actions()),
            (exit, owned.iteration_cleanup_at(exit.point())),
        );
    }
    let Some(UnitLoanTarget::Place(place)) = plan.source() else {
        report.check("unit source cleanup root exists", false, plan.source());
        return;
    };
    let source = place.root();
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(source))
        .collect::<Vec<_>>();
    if !is_owned {
        report.check(
            "unit Borrow source has no owner drop",
            drops.is_empty()
                && end.actions() == [UA::FinishProvider(statement), UA::EndSource(statement)],
            (drops, end),
        );
        return;
    }
    report.check(
        "unit direct Map own single source-qualified drop",
        drops.len() == 1,
        &drops,
    );
    let [drop] = drops.as_slice() else {
        return;
    };
    report.check(
        "unit direct Map own only LoopExit drop after source end",
        drop.point() == end.point()
            && end.actions()
                == [
                    UA::FinishProvider(statement),
                    UA::EndSource(statement),
                    UA::Drop(**drop),
                ],
        (drop, end),
    );
    // 原 owner/phi/incoming/条件/无 capture 义务由同轮 group::ssa 的真实图承接。
    // 不构造旧 FE 表，也不把同型实体视为同一 owner。
}
