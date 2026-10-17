//! 当前 ownership 诊断、非 owner binding 与原 DropFact 查询。
//! 不将全局 binding_kind(None) 视为 Shared，不由 drops 推断 provider/loan 动作顺序。
#[path = "source_owner_phi.rs"]
mod source_owner_phi;

use super::{
    Report,
    cases::{Case, Expected, SourceKind},
    fixture::{Fixture, Sites, diagnostics},
    type_facts::{single_reference, symbol_at, unit_reference, unit_symbol},
};
use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{NameResolution, ValidatedCompilationUnitNames},
    ownership_checking::{
        CleanupOwnerValue, CompilationUnitOwnership, DropPoint, DropTarget, OwnershipBindingKind,
        OwnershipCheckedFile, UnitDropPoint, UnitDropTarget,
    },
    type_checking::{CompilationUnitTypes, TypedFile, UnitExpressionId, UnitStatementId},
};

fn diagnostic_target(case: &Case, items: &[Diagnostic]) -> bool {
    match case.expected {
        Expected::Accepted(_) => items.is_empty(),
        Expected::Rejected(code) => items.len() == 1 && items[0].code().to_string() == code,
    }
}

pub(super) fn single(
    case: &Case,
    fixture: &Fixture,
    sites: &Sites,
    names: &NameResolution,
    raw: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    report.record(
        "ownership original identity",
        owned.is_compatible_with(names, raw),
        owned.source_id(),
    );
    report.record(
        "ownership diagnostic target",
        diagnostic_target(case, owned.diagnostics()),
        diagnostics(&fixture.sources, "ownership", owned.diagnostics()),
    );
    report.record(
        "ownership deferred",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    report.note("original ownership drops", owned.drops());
    report.note("formed owned locals", owned.formed_owned_locals());
    let Expected::Accepted(source_kind) = case.expected else {
        return;
    };
    for span in sites.bindings {
        let symbol = symbol_at(names, span).map(|symbol| symbol.id());
        report.record(
            "component not formed owner",
            symbol.is_some_and(|symbol| !owned.formed_owned_locals().contains(&symbol)),
            symbol,
        );
        report.record(
            "component no named drop",
            symbol.is_some_and(|symbol| {
                owned
                    .drops()
                    .iter()
                    .all(|fact| fact.target() != DropTarget::Named(symbol))
            }),
            symbol,
        );
        report.note(
            "component global binding kind (not Shared oracle)",
            symbol.map(|symbol| owned.binding_kind(symbol)),
        );
    }
    if let Some((span, initializer)) = sites.local {
        let local = symbol_at(names, span).map(|symbol| symbol.id());
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| local.is_some_and(|symbol| fact.target() == DropTarget::Named(symbol)))
            .collect::<Vec<_>>();
        let valid = local.is_some_and(|symbol| owned.formed_owned_locals().contains(&symbol))
            && !drops.is_empty() && drops.iter().all(|fact| {
                fact.owner().and_then(|owner| owned.cleanup_conditions().owner_value(owner))
                    .is_some_and(|value| matches!(value, CleanupOwnerValue::Expression { expression, .. } if *expression == initializer))
            });
        // 不冻结 local 的 ASAP DropPoint；只追溯其真实 Guard() owner。
        report.record(
            "body local real owner cleanup",
            valid,
            (local, initializer, drops),
        );
    }
    match source_kind {
        SourceKind::Owned | SourceKind::Borrowed => {
            let source = single_reference(names, sites.source_span);
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| {
                    source.is_some_and(|symbol| fact.target() == DropTarget::Named(symbol))
                })
                .collect::<Vec<_>>();
            let kind = source.and_then(|source| owned.binding_kind(source));
            match source_kind {
                SourceKind::Owned => {
                    let source_owner = source_owner_phi::query(fixture, sites, names, owned);
                    let valid = kind == Some(OwnershipBindingKind::Owned)
                        && !drops.is_empty()
                        && source_owner.is_ok();
                    report.note("owned named source grounded phi identity", &source_owner);
                    // 仅诊断实际 source drop 的 owner/condition 与同 source 的 phi 入边。
                    let conditions = owned.cleanup_conditions();
                    report.note("owned named source cleanup condition table", conditions);
                    for fact in &drops {
                        report.note(
                            "owned named source drop owner/condition",
                            (
                                fact,
                                fact.owner().and_then(|owner| conditions.owner_value(owner)),
                                fact.condition()
                                    .and_then(|condition| conditions.get(condition)),
                            ),
                        );
                    }
                    let plan = owned.map_iteration(sites.statement);
                    report.note(
                        "owned named source actual Map plan",
                        (sites.statement, source, plan.is_some()),
                    );
                    if let Some(plan) = plan {
                        let phis = plan
                            .closure_phis()
                            .iter()
                            .filter(|phi| Some(phi.symbol()) == source)
                            .collect::<Vec<_>>();
                        report.note("owned named source matching phis", &phis);
                        for incoming in plan.closure_phi_incomings() {
                            for binding in incoming.bindings().iter().filter(|binding| {
                                phis.iter().any(|phi| phi.owner() == binding.target())
                            }) {
                                let values = binding
                                    .values()
                                    .iter()
                                    .map(|value| (value, conditions.owner_value(value.source())))
                                    .collect::<Vec<_>>();
                                report.note(
                                    "owned named source phi incoming",
                                    (
                                        incoming.kind(),
                                        incoming.point(),
                                        incoming.boundary(),
                                        incoming.condition(),
                                        binding,
                                        values,
                                    ),
                                );
                            }
                        }
                    }
                    // 只核本 fixture 的参数根与实际 LoopExit；不泛化任意 phi 或 ASAP 点。
                    report.record(
                        "owned named source final cleanup",
                        valid,
                        (source, kind, drops),
                    );
                }
                SourceKind::Borrowed => {
                    let valid = kind == Some(OwnershipBindingKind::Shared)
                        && drops.is_empty()
                        && owned
                            .drops()
                            .iter()
                            .all(|fact| fact.target() != DropTarget::Temporary(sites.source));
                    report.record(
                        "borrowed source retained through continue",
                        valid,
                        (source, kind, drops, sites.transfer),
                    );
                }
                SourceKind::Temporary => unreachable!(),
            }
        }
        SourceKind::Temporary => {
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.target() == DropTarget::Temporary(sites.source))
                .collect::<Vec<_>>();
            let exhaustion = drops
                .iter()
                .filter(|fact| fact.point() == DropPoint::LoopExit(sites.statement))
                .count();
            let terminal = drops
                .iter()
                .filter(|fact| {
                    sites.transfer.is_some_and(|transfer| {
                        fact.point() == DropPoint::ControlTransfer(transfer)
                    })
                })
                .count();
            let owners_match = drops.iter().all(|fact| fact.owner()
                .and_then(|owner| owned.cleanup_conditions().owner_value(owner))
                .is_some_and(|value| matches!(value, CleanupOwnerValue::Expression { expression, .. } if *expression == sites.source)));
            report.record(
                "temporary source exhaustion/terminal cleanup",
                drops.len() == 2 && exhaustion == 1 && terminal == 1 && owners_match,
                (sites.source, exhaustion, terminal, drops),
            );
        }
    }
}

pub(super) fn unit(
    case: &Case,
    fixture: &Fixture,
    sites: &Sites,
    names: &ValidatedCompilationUnitNames,
    raw: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    report.record(
        "ownership diagnostic target",
        diagnostic_target(case, owned.diagnostics()),
        diagnostics(&fixture.sources, "ownership", owned.diagnostics()),
    );
    report.record(
        "ownership deferred",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    report.note("original ownership drops", owned.drops());
    report.note("formed owned locals", owned.formed_owned_locals());
    let Expected::Accepted(source_kind) = case.expected else {
        return;
    };
    let [source_names] = names.names().source_units() else {
        report.record(
            "unit source identity",
            false,
            names.names().source_units().len(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let local_names = source_names.resolution();
    for span in sites.bindings {
        let symbol =
            symbol_at(local_names, span).and_then(|symbol| unit_symbol(raw, unit, symbol.id()));
        report.record(
            "component not formed owner",
            symbol.is_some_and(|symbol| !owned.formed_owned_locals().contains(&symbol)),
            symbol,
        );
        report.record(
            "component no named drop",
            symbol.is_some_and(|symbol| {
                owned
                    .drops()
                    .iter()
                    .all(|fact| fact.target() != UnitDropTarget::Named(symbol))
            }),
            symbol,
        );
        report.note(
            "component global binding kind (not Shared oracle)",
            symbol.map(|symbol| owned.binding_kind(symbol)),
        );
    }
    if let Some((span, initializer)) = sites.local {
        let local =
            symbol_at(local_names, span).and_then(|symbol| unit_symbol(raw, unit, symbol.id()));
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| {
                local.is_some_and(|symbol| fact.target() == UnitDropTarget::Named(symbol))
            })
            .collect::<Vec<_>>();
        // UnitDropFact 没有 owner() API；真实 Named 身份与建立来源才是现有可查询合同。
        let valid = local.is_some_and(|symbol| owned.formed_owned_locals().contains(&symbol))
            && !drops.is_empty()
            && drops.iter().all(|fact| fact.value_origin() == span);
        report.record(
            "body local unit identity/origin cleanup",
            valid,
            (local, initializer, drops),
        );
    }
    let source_expression = UnitExpressionId::new(unit, sites.source);
    match source_kind {
        SourceKind::Owned | SourceKind::Borrowed => {
            let source = unit_reference(names, sites.source_span);
            let origin = source
                .and_then(|source| {
                    local_names.symbols().iter().find(|symbol| {
                        source.source_unit() == unit && symbol.id() == source.symbol()
                    })
                })
                .map(|symbol| symbol.span());
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| {
                    source.is_some_and(|symbol| fact.target() == UnitDropTarget::Named(symbol))
                })
                .collect::<Vec<_>>();
            let kind = source.and_then(|source| owned.binding_kind(source));
            match source_kind {
                SourceKind::Owned => {
                    let valid = kind == Some(OwnershipBindingKind::Owned)
                        && !drops.is_empty()
                        && drops.iter().all(|fact| Some(fact.value_origin()) == origin);
                    report.record(
                        "owned named unit source final cleanup",
                        valid,
                        (source, kind, origin, drops),
                    );
                }
                SourceKind::Borrowed => {
                    let valid = kind == Some(OwnershipBindingKind::Shared)
                        && drops.is_empty()
                        && owned.drops().iter().all(|fact| {
                            fact.target() != UnitDropTarget::Temporary(source_expression)
                        });
                    report.record(
                        "borrowed unit source retained through continue",
                        valid,
                        (source, kind, drops, sites.transfer),
                    );
                }
                SourceKind::Temporary => unreachable!(),
            }
        }
        SourceKind::Temporary => {
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.target() == UnitDropTarget::Temporary(source_expression))
                .collect::<Vec<_>>();
            let statement = UnitStatementId::new(unit, sites.statement);
            let exhaustion = drops
                .iter()
                .filter(|fact| fact.point() == UnitDropPoint::LoopExit(statement))
                .count();
            let terminal = drops
                .iter()
                .filter(|fact| {
                    sites.transfer.is_some_and(|transfer| {
                        fact.point()
                            == UnitDropPoint::ControlTransfer(UnitExpressionId::new(unit, transfer))
                    })
                })
                .count();
            report.record(
                "temporary unit source exhaustion/terminal cleanup",
                drops.len() == 2
                    && exhaustion == 1
                    && terminal == 1
                    && drops
                        .iter()
                        .all(|fact| fact.value_origin() == sites.source_span),
                (source_expression, exhaustion, terminal, drops),
            );
        }
    }
}
