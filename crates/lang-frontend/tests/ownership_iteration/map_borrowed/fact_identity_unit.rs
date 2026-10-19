//! Unit 独立 query 保留所有 source-qualified identity；不伪造 condition/owner API。
use super::super::{
    cases::{Expected, SourceKind},
    type_facts::{symbol_at, unit_reference, unit_symbol},
};
use super::{BodyExit, Case, Fixture, QueryResult, Sites, body_exit, map_kind, require};
use lang_frontend::{
    name_resolution::{SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent as Component,
        OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as Action,
        UnitIterationExitKind as Exit, UnitIterationSourceAccess, UnitLoanTarget,
        UnitMapIterationOwnershipPlan,
    },
    type_checking::{CompilationUnitTypes, UnitExpressionId, UnitStatementId, UnitTypeKind},
};

pub(super) fn query(
    case: &Case,
    fixture: &Fixture,
    sites: &Sites,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> QueryResult {
    require(
        typed.diagnostics().is_empty()
            && owned.diagnostics().is_empty()
            && owned.deferred().is_empty(),
        "unit query requires clean original Phase 2/3 products and original basic validation",
    )?;
    let [source_names] = names.names().source_units() else {
        return Err("fixture must retain exactly one actual source unit");
    };
    let unit = source_names.source_unit();
    let local_names = source_names.resolution();
    require(
        local_names.source_id() == fixture.file.source_id(),
        "unit names are not for the actual parsed source",
    )?;
    let statement = UnitStatementId::new(unit, sites.statement);
    let source = UnitExpressionId::new(unit, sites.source);
    let [descriptor] = typed.map_iterations() else {
        return Err("unit typed table must contain exactly the actual Map For");
    };
    require(
        typed.map_iteration(statement) == Some(descriptor)
            && typed.sequential_iteration(statement).is_none()
            && descriptor.statement() == statement
            && descriptor.source() == source
            && descriptor.statement().source_unit() == unit
            && descriptor.source().source_unit() == unit
            && typed.expression_type(source) == Some(descriptor.source_type())
            && descriptor.kind() == map_kind(case)?,
        "unit typed Map source/For/kind identity mismatch",
    )?;
    let Some(UnitTypeKind::Intrinsic {
        constructor,
        arguments,
    }) = typed.types().get(descriptor.source_type())
    else {
        return Err("unit typed Map source is not an intrinsic type");
    };
    let [key_type, value_type] = arguments.as_slice() else {
        return Err("unit intrinsic Map must have exactly K and V");
    };
    require(
        *constructor == case.constructor
            && descriptor.key_type() == *key_type
            && descriptor.value_type() == *value_type,
        "unit descriptor changed intrinsic K/V identities",
    )?;
    let mut symbols = Vec::new();
    for (index, expected_type) in [*key_type, *value_type].into_iter().enumerate() {
        let local = symbol_at(local_names, sites.bindings[index])
            .ok_or("missing unique real local binding")?;
        let symbol = unit_symbol(typed, unit, local.id())
            .ok_or("missing original source-qualified binding")?;
        require(
            local.kind() == SymbolKind::ForBinding
                && symbol.source_unit() == unit
                && symbol.symbol() == local.id()
                && typed.symbol_type(symbol) == Some(expected_type)
                && typed.types().get(expected_type) == Some(&UnitTypeKind::Builtin(case.component)),
            "actual unit binding has wrong identity or component type",
        )?;
        for (expression, span) in &sites.reads[index] {
            require(
                unit_reference(names, *span) == Some(symbol)
                    && typed.expression_type(UnitExpressionId::new(unit, *expression))
                        == Some(expected_type),
                "actual unit component read has wrong symbol or K/V type",
            )?;
        }
        symbols.push(symbol);
    }
    let [key, value] = symbols.as_slice() else {
        return Err("missing two unit binding symbols");
    };
    let bindings = [*key, *value];
    require(
        key != value
            && descriptor.bindings() == bindings
            && descriptor.key_binding() == *key
            && descriptor.value_binding() == *value,
        "unit typed key/value bindings are swapped, identical, or foreign",
    )?;
    let [plan] = owned.map_iterations() else {
        return Err("unit ownership must have one Map plan");
    };
    require(
        owned.map_iteration(statement) == Some(plan)
            && owned.iteration(statement).is_none()
            && owned.iterations().is_empty()
            && plan.descriptor() == descriptor,
        "unit ownership Map descriptor differs from original typed descriptor",
    )?;
    let IterationSource::BorrowedSource(origin) = plan.source_kind() else {
        return Err("unit Map source must be a borrowed lease, not an owned snapshot");
    };
    require(
        plan.source() == Some(origin),
        "unit Map source getter differs from source-kind origin",
    )?;
    let Expected::Accepted(source_kind) = case.expected else {
        return Err("unit query only accepts P1-P4");
    };
    match source_kind {
        SourceKind::Owned | SourceKind::Borrowed => {
            let named = unit_reference(names, sites.source_span)
                .ok_or("missing actual qualified named source")?;
            let access = match source_kind {
                SourceKind::Owned => UnitIterationSourceAccess::Owned,
                _ => UnitIterationSourceAccess::Shared,
            };
            require(
                named.source_unit() == unit
                    && plan.source_access() == access
                    && matches!(origin, UnitLoanTarget::Place(place) if place.is_root() && place.root() == named),
                "unit Map lease is not the exact qualified named root/capability",
            )?;
        }
        SourceKind::Temporary => require(
            plan.source_access() == UnitIterationSourceAccess::Temporary
                && origin == &UnitLoanTarget::Temporary(source),
            "unit Map source is not the actual qualified factory temporary",
        )?,
    }
    for (index, component) in [Component::Key, Component::Value].into_iter().enumerate() {
        let symbol = bindings[index];
        let binding = &plan.bindings()[index];
        let loan = &plan.loans()[index];
        require(
            binding.symbol() == symbol
                && binding.kind() == OwnershipBindingKind::Shared
                && binding.declaration_span() == sites.bindings[index]
                && owned.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
                && !owned.formed_owned_locals().contains(&symbol)
                && owned
                    .drops()
                    .iter()
                    .all(|fact| fact.target() != UnitDropTarget::Named(symbol))
                && plan.loan(component) == loan
                && loan.component() == component
                && loan.binding() == symbol
                && loan.statement() == statement
                && loan.source() == source
                && loan.origin() == origin
                && loan.kind() == LoanKind::Shared,
            "unit Map Shared binding/loan source-qualified lineage mismatch",
        )?;
    }
    let expression = |id| UnitExpressionId::new(unit, id);
    let body = match body_exit(fixture, sites)? {
        BodyExit::Fallthrough(last) => (
            Exit::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, last)),
        ),
        BodyExit::Continue(id) => (
            Exit::Continue(expression(id)),
            UnitDropPoint::ControlTransfer(expression(id)),
        ),
        BodyExit::Break(id) => (
            Exit::Break(expression(id)),
            UnitDropPoint::ControlTransfer(expression(id)),
        ),
        BodyExit::Return(id) => (
            Exit::Return(expression(id)),
            UnitDropPoint::ControlTransfer(expression(id)),
        ),
    };
    let targets = [body, (Exit::Exhaustion, UnitDropPoint::LoopExit(statement))];
    require(
        plan.exits().len() == targets.len() && targets[0].1 != targets[1].1,
        "unit expected exactly actual body edge and separate exhaustion edge",
    )?;
    let local = sites
        .local
        .and_then(|(span, _)| symbol_at(local_names, span))
        .and_then(|symbol| unit_symbol(typed, unit, symbol.id()));
    for (kind, point) in targets {
        let matches = plan
            .exits()
            .iter()
            .filter(|exit| exit.kind() == kind && exit.point() == point)
            .collect::<Vec<_>>();
        let [exit] = matches.as_slice() else {
            return Err("missing or duplicate exact qualified AST exit kind/point");
        };
        require(
            owned.iteration_cleanup_at(point) == Some(exit.actions()),
            "unit exit does not reference the complete same-point cleanup",
        )?;
        cleanup(sites, owned, plan, kind, point, exit.actions(), local)?;
    }
    Ok(())
}

fn is_map_action(action: &Action, statement: UnitStatementId) -> bool {
    match action {
        Action::EndBinding {
            statement: actual, ..
        }
        | Action::EndElement(actual)
        | Action::FinishProvider(actual)
        | Action::EndSource(actual) => *actual == statement,
        _ => false,
    }
}

fn cleanup(
    sites: &Sites,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    kind: Exit,
    point: UnitDropPoint,
    actions: &[Action],
    local: Option<UnitSymbolId>,
) -> QueryResult {
    let statement = plan.descriptor().statement();
    let source = plan.descriptor().source();
    let [key, value] = plan.descriptor().bindings();
    let exhausted = kind == Exit::Exhaustion;
    let terminal = matches!(kind, Exit::Break(_) | Exit::Return(_) | Exit::Exhaustion);
    let mut expected = Vec::new();
    if !exhausted {
        expected.push(Action::EndBinding {
            statement,
            symbol: value,
        });
        expected.push(Action::EndBinding {
            statement,
            symbol: key,
        });
    }
    if terminal {
        expected.push(Action::FinishProvider(statement));
        expected.push(Action::EndSource(statement));
    }
    let actual = actions
        .iter()
        .filter(|action| is_map_action(action, statement))
        .collect::<Vec<_>>();
    require(
        actual == expected.iter().collect::<Vec<_>>(),
        "unit Map cleanup must be exact value/key then provider/source, no EndElement or foreign binding",
    )?;
    let drops = actions
        .iter()
        .filter_map(|action| match action {
            Action::Drop(fact) => Some(fact),
            _ => None,
        })
        .collect::<Vec<_>>();
    let recorded = owned
        .drops()
        .iter()
        .filter(|fact| fact.point() == point)
        .collect::<Vec<_>>();
    require(
        drops == recorded,
        "unit same-point Drop identities/order differ from original drop facts",
    )?;
    let binding_end = actions.iter().position(|action| {
        *action
            == Action::EndBinding {
                statement,
                symbol: value,
            }
    });
    let source_end = actions
        .iter()
        .position(|action| *action == Action::EndSource(statement));
    for (index, action) in actions.iter().enumerate() {
        require(
            !actions[..index].contains(action),
            "unit same-point cleanup contains duplicate complete action",
        )?;
        match action {
            Action::Drop(fact) => {
                if matches!(fact.target(), UnitDropTarget::Named(symbol) if Some(symbol) == local) {
                    require(
                        binding_end.is_some_and(|end| index < end),
                        "unit same-point body local must drop before components",
                    )?;
                }
                let source_drop = match plan.source() {
                    Some(UnitLoanTarget::Place(place)) => {
                        fact.target() == UnitDropTarget::Named(place.root())
                    }
                    Some(UnitLoanTarget::Temporary(source)) => {
                        fact.target() == UnitDropTarget::Temporary(*source)
                    }
                    _ => false,
                };
                if source_drop {
                    require(
                        terminal && source_end.is_some_and(|end| index > end),
                        "unit source owner dropped before provider/source end",
                    )?;
                }
            }
            Action::EndCallLoan(_) | Action::EndReceiverLoan(_) | Action::EndCaptureLoan { .. } => {
                require(
                    binding_end.is_some_and(|end| index < end),
                    "unit derived loan/capture must end before components",
                )?;
            }
            _ => {}
        }
    }
    if plan.source_access() == UnitIterationSourceAccess::Temporary {
        let source_drops = drops
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Temporary(source))
            .collect::<Vec<_>>();
        require(
            source_drops.len() == usize::from(terminal),
            "unit temporary missing/duplicate same-edge cleanup",
        )?;
        for fact in source_drops {
            require(
                fact.value_origin() == sites.source_span && fact.point() == point,
                "unit temporary cleanup lost qualified factory/point/origin identity",
            )?;
        }
        require(
            owned
                .drops()
                .iter()
                .filter(|fact| fact.target() == UnitDropTarget::Temporary(source))
                .count()
                == 2,
            "unit temporary must have separate terminal/exhaustion Drops",
        )?;
    }
    Ok(())
}
