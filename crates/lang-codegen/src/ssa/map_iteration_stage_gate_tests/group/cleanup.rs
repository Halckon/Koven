//! 完整 normal 清理与 direct-own LoopExit owner 旁证；只读，不构造事实。
use super::super::fixture::{File, LoopSite};
use super::Report;
use lang_frontend::{
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CompilationUnitOwnership, DropPoint, DropTarget,
        IterationCleanupAction as A, IterationExitKind, IterationPhiBoundary,
        IterationPhiIncomingKind, LoanTarget, MapIterationOwnershipPlan, OwnershipCheckedFile,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA, UnitIterationExitKind,
        UnitLoanTarget, UnitMapIterationOwnershipPlan,
    },
    parser::Statement,
    type_checking::UnitStatementId,
};

pub(super) fn single(
    file: &File,
    site: &LoopSite,
    plan: &MapIterationOwnershipPlan,
    owned: &OwnershipCheckedFile,
    is_owned: bool,
    report: &mut Report,
) {
    let body = file
        .parsed
        .ast()
        .statements()
        .get(site.statement)
        .ok()
        .and_then(|node| {
            if let Statement::For { body, .. } = node.payload() {
                Some(*body)
            } else {
                None
            }
        });
    let falls = plan
        .exits()
        .iter()
        .filter(|exit| exit.kind() == IterationExitKind::Fallthrough)
        .collect::<Vec<_>>();
    let ends = plan
        .exits()
        .iter()
        .filter(|exit| exit.kind() == IterationExitKind::Exhaustion)
        .collect::<Vec<_>>();
    report.check(
        "single exactly normal body and exhaustion",
        plan.exits().len() == 2 && falls.len() == 1 && ends.len() == 1,
        plan.exits(),
    );
    let ([fall], [end]) = (falls.as_slice(), ends.as_slice()) else {
        return;
    };
    let descriptor = plan.descriptor();
    report.check(
        "single full normal body actions and point",
        Some(fall.point()) == body.map(DropPoint::AfterStatement)
            && fall.actions()
                == [
                    A::EndBinding {
                        statement: site.statement,
                        symbol: descriptor.value_binding(),
                    },
                    A::EndBinding {
                        statement: site.statement,
                        symbol: descriptor.key_binding(),
                    },
                ]
            && end.point() == DropPoint::LoopExit(site.statement),
        (fall, end),
    );
    for exit in [fall, end] {
        let actions = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| (*point == exit.point()).then_some(*action))
            .collect::<Vec<_>>();
        report.check(
            "single complete same-point cleanup actions",
            actions == exit.actions(),
            (exit, actions),
        );
    }
    let Some(LoanTarget::Place(place)) = plan.source() else {
        report.check("single source cleanup root exists", false, plan.source());
        return;
    };
    let source = place.root();
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Named(source))
        .collect::<Vec<_>>();
    if !is_owned {
        report.check(
            "single Borrow source no owner drop or phis",
            drops.is_empty()
                && plan.closure_phis().is_empty()
                && end.actions()
                    == [
                        A::FinishProvider(site.statement),
                        A::EndSource(site.statement),
                    ],
            (drops, plan.closure_phis(), end),
        );
        return;
    }
    let headers = plan
        .closure_phis()
        .iter()
        .filter(|phi| phi.symbol() == source && phi.boundary() == IterationPhiBoundary::Header)
        .collect::<Vec<_>>();
    let exits = plan
        .closure_phis()
        .iter()
        .filter(|phi| phi.symbol() == source && phi.boundary() == IterationPhiBoundary::Exit)
        .collect::<Vec<_>>();
    report.check(
        "single direct Map own actual LoopExit owner topology",
        drops.len() == 1
            && plan.closure_phis().len() == 2
            && headers.len() == 1
            && exits.len() == 1
            && plan.closure_phi_incomings().len() == 3
            && plan.capture_graph().nodes().is_empty(),
        (&drops, &headers, &exits, plan.closure_phi_incomings()),
    );
    let ([drop], [header], [exit]) = (drops.as_slice(), headers.as_slice(), exits.as_slice())
    else {
        return;
    };
    report.check(
        "single owner drop only at exhaustion after source end",
        drop.point() == end.point()
            && drop.owner() == Some(header.owner())
            && drop.condition() == Some(header.availability_condition())
            && header.owner() != exit.owner()
            && end.actions()
                == [
                    A::FinishProvider(site.statement),
                    A::EndSource(site.statement),
                    A::Drop(**drop),
                ],
        (drop, header, exit, end),
    );
    let conditions = owned.cleanup_conditions();
    let mut kinds = [0; 3];
    for incoming in plan.closure_phi_incomings() {
        let [binding] = incoming.bindings() else {
            report.check(
                "single actual source-owner incoming binding",
                false,
                incoming,
            );
            continue;
        };
        let passed = match incoming.kind() {
            IterationPhiIncomingKind::Entry => {
                kinds[0] += 1;
                incoming.boundary() == IterationPhiBoundary::Header
                    && incoming.point() == DropPoint::AfterExpression(site.source)
                    && binding.target() == header.owner()
                    && conditions.get(binding.available_when()) == Some(&CleanupCondition::Always)
                    && matches!(binding.values(), [value] if value.condition() == binding.available_when()
                        && matches!(conditions.owner_value(value.source()), Some(CleanupOwnerValue::Parameter { symbol, origin })
                            if *symbol == source && *origin == drop.value_origin()))
            }
            IterationPhiIncomingKind::Fallthrough => {
                kinds[1] += 1;
                incoming.boundary() == IterationPhiBoundary::Header
                    && incoming.point() == fall.point()
                    && binding.target() == header.owner()
                    && binding.available_when() == header.availability_condition()
                    && matches!(binding.values(), [value] if value.source() == header.owner()
                        && value.condition() == header.availability_condition())
            }
            IterationPhiIncomingKind::Exhaustion => {
                kinds[2] += 1;
                incoming.boundary() == IterationPhiBoundary::Exit
                    && incoming.point() == end.point()
                    && binding.target() == exit.owner()
                    && binding.values().is_empty()
                    && conditions.get(binding.available_when()) == Some(&CleanupCondition::Never)
            }
            _ => false,
        };
        report.check(
            "single owner incoming real identity/point/condition",
            passed,
            incoming,
        );
    }
    report.check(
        "single every owner edge exactly once",
        kinds == [1, 1, 1],
        kinds,
    );
}

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
    // unit 公共事实不发布 single closure owner phi 表，不跨阶段伪造它。
}
