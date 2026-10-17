//! 单文件的独立只读身份查询；成功目标与原行为 oracle 同时保留。
use super::super::{
    cases::{Expected, SourceKind},
    type_facts::{single_reference, symbol_at},
};
use super::{BodyExit, Case, Fixture, QueryResult, Sites, body_exit, map_kind, require};
use lang_frontend::{
    ast::StatementId,
    name_resolution::{NameResolution, SymbolKind},
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, DropPoint, DropTarget,
        IterationCleanupAction as Action, IterationExitKind as Exit, IterationSource, LoanKind,
        LoanTarget, MapIterationComponent as Component, MapIterationOwnershipPlan,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
    },
    type_checking::{TypeKind, TypedFile},
};

pub(super) fn query(
    case: &Case,
    fixture: &Fixture,
    sites: &Sites,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> QueryResult {
    require(
        typed.diagnostics().is_empty()
            && owned.diagnostics().is_empty()
            && owned.deferred().is_empty()
            && owned.is_compatible_with(names, typed),
        "single query requires original clean compatible Phase 2/3 products",
    )?;
    let [descriptor] = typed.map_iterations() else {
        return Err("single typed table must contain exactly the actual Map For");
    };
    require(
        typed.map_iteration(sites.statement) == Some(descriptor)
            && typed.sequential_iteration(sites.statement).is_none()
            && descriptor.statement() == sites.statement
            && descriptor.source() == sites.source
            && typed.expression_type(sites.source) == Some(descriptor.source_type())
            && descriptor.kind() == map_kind(case)?,
        "typed Map source/For/kind identity mismatch",
    )?;
    let Some(TypeKind::Intrinsic {
        constructor,
        arguments,
    }) = typed.types().get(descriptor.source_type())
    else {
        return Err("typed Map source is not an intrinsic type");
    };
    let [key_type, value_type] = arguments.as_slice() else {
        return Err("intrinsic Map must have exactly K and V");
    };
    require(
        *constructor == case.constructor
            && descriptor.key_type() == *key_type
            && descriptor.value_type() == *value_type,
        "typed descriptor changed intrinsic K/V identities",
    )?;
    let mut symbols = Vec::new();
    for (index, expected_type) in [*key_type, *value_type].into_iter().enumerate() {
        let symbol = symbol_at(names, sites.bindings[index])
            .ok_or("missing unique actual binding symbol")?;
        require(
            symbol.kind() == SymbolKind::ForBinding
                && typed.symbol_type(symbol.id()) == Some(expected_type)
                && typed.types().get(expected_type) == Some(&TypeKind::Builtin(case.component)),
            "actual Map binding has wrong identity or component type",
        )?;
        for (expression, span) in &sites.reads[index] {
            require(
                single_reference(names, *span) == Some(symbol.id())
                    && typed.expression_type(*expression) == Some(expected_type),
                "actual component read has wrong symbol or K/V type",
            )?;
        }
        symbols.push(symbol.id());
    }
    let [key, value] = symbols.as_slice() else {
        return Err("missing two binding symbols");
    };
    let bindings = [*key, *value];
    require(
        key != value
            && descriptor.bindings() == bindings
            && descriptor.key_binding() == *key
            && descriptor.value_binding() == *value,
        "typed key/value bindings are swapped, identical, or foreign",
    )?;
    let [plan] = owned.map_iterations() else {
        return Err("single ownership must have one Map plan");
    };
    require(
        owned.map_iteration(sites.statement) == Some(plan)
            && owned.iteration(sites.statement).is_none()
            && owned.iterations().is_empty()
            && plan.descriptor() == descriptor,
        "ownership Map descriptor differs from actual typed descriptor",
    )?;
    let IterationSource::BorrowedSource(origin) = plan.source_kind() else {
        return Err("Map source must be a borrowed lease, not an owned snapshot");
    };
    require(
        plan.source() == Some(origin),
        "Map source getter differs from source-kind origin",
    )?;
    let Expected::Accepted(source_kind) = case.expected else {
        return Err("query only accepts P1-P4");
    };
    match source_kind {
        SourceKind::Owned | SourceKind::Borrowed => {
            let source =
                single_reference(names, sites.source_span).ok_or("missing real named source")?;
            let access = match source_kind {
                SourceKind::Owned => MapIterationSourceAccess::Owned,
                _ => MapIterationSourceAccess::Shared,
            };
            require(
                plan.source_access() == access
                    && matches!(origin, LoanTarget::Place(place) if place.is_root() && place.root() == source),
                "Map source lease is not the exact named root/capability",
            )?;
        }
        SourceKind::Temporary => require(
            plan.source_access() == MapIterationSourceAccess::Temporary
                && origin == &LoanTarget::Temporary(sites.source),
            "Map source is not the actual factory temporary",
        )?,
    }
    for (index, component) in [Component::Key, Component::Value].into_iter().enumerate() {
        let symbol = bindings[index];
        let binding = &plan.bindings()[index];
        let loan = &plan.loans()[index];
        require(
            binding.symbol() == symbol
                && binding.kind() == OwnershipBindingKind::Shared
                && owned.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
                && !owned.formed_owned_locals().contains(&symbol)
                && owned
                    .drops()
                    .iter()
                    .all(|fact| fact.target() != DropTarget::Named(symbol))
                && plan.loan(component) == loan
                && loan.component() == component
                && loan.binding() == symbol
                && loan.statement() == sites.statement
                && loan.source() == sites.source
                && loan.origin() == origin
                && loan.kind() == LoanKind::Shared,
            "Map component Shared binding/loan lineage mismatch",
        )?;
    }
    let body = match body_exit(fixture, sites)? {
        BodyExit::Fallthrough(statement) => {
            (Exit::Fallthrough, DropPoint::AfterStatement(statement))
        }
        BodyExit::Continue(expression) => (
            Exit::Continue(expression),
            DropPoint::ControlTransfer(expression),
        ),
        BodyExit::Break(expression) => (
            Exit::Break(expression),
            DropPoint::ControlTransfer(expression),
        ),
        BodyExit::Return(expression) => (
            Exit::Return(expression),
            DropPoint::ControlTransfer(expression),
        ),
    };
    let targets = [
        body,
        (Exit::Exhaustion, DropPoint::LoopExit(sites.statement)),
    ];
    require(
        plan.exits().len() == targets.len() && targets[0].1 != targets[1].1,
        "expected exactly actual body edge and separate exhaustion edge",
    )?;
    for (kind, point) in targets {
        let matches = plan
            .exits()
            .iter()
            .filter(|exit| exit.kind() == kind && exit.point() == point)
            .collect::<Vec<_>>();
        let [exit] = matches.as_slice() else {
            return Err("missing or duplicate exact AST exit kind/point");
        };
        require(
            owned.cleanup_conditions().get(exit.condition()) == Some(&CleanupCondition::Always),
            "unconditional fixture exit must retain its actual Always path",
        )?;
        let complete = owned
            .cleanup_steps()
            .iter()
            .filter(|(actual, _)| *actual == point)
            .map(|(_, action)| *action)
            .collect::<Vec<_>>();
        require(
            exit.actions() == complete.as_slice(),
            "exit does not reference complete same-point cleanup exactly once",
        )?;
        cleanup(sites, names, owned, plan, kind, point, exit.actions())?;
    }
    for (point, action) in owned.cleanup_steps() {
        if is_map_action(action, sites.statement) {
            require(
                targets
                    .iter()
                    .filter(|(_, expected)| expected == point)
                    .count()
                    == 1,
                "orphan Map action outside the two exact actual exit points",
            )?;
        }
    }
    Ok(())
}

fn is_map_action(action: &Action, statement: StatementId) -> bool {
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
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    kind: Exit,
    point: DropPoint,
    actions: &[Action],
) -> QueryResult {
    let statement = sites.statement;
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
        .copied()
        .filter(|action| is_map_action(action, statement))
        .collect::<Vec<_>>();
    require(
        actual == expected,
        "Map cleanup must be exact value/key then provider/source, with no EndElement or foreign binding",
    )?;
    let drops = actions
        .iter()
        .filter_map(|action| match action {
            Action::Drop(fact) => Some(*fact),
            _ => None,
        })
        .collect::<Vec<_>>();
    let recorded = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.point() == point)
        .collect::<Vec<_>>();
    require(
        drops == recorded,
        "same-point Drop identities/order differ from original drop facts",
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
    let local = sites
        .local
        .and_then(|(span, _)| symbol_at(names, span))
        .map(|symbol| symbol.id());
    for (index, action) in actions.iter().enumerate() {
        require(
            !actions[..index].contains(action),
            "same-point cleanup contains a duplicate complete action",
        )?;
        match action {
            Action::Drop(fact) => {
                if matches!(fact.target(), DropTarget::Named(symbol) if Some(symbol) == local) {
                    require(
                        binding_end.is_some_and(|end| index < end),
                        "same-point body local must drop before component ends",
                    )?;
                }
                let source_drop = match plan.source() {
                    Some(LoanTarget::Place(place)) => {
                        fact.target() == DropTarget::Named(place.root())
                    }
                    Some(LoanTarget::Temporary(source)) => {
                        fact.target() == DropTarget::Temporary(*source)
                    }
                    _ => false,
                };
                if source_drop {
                    require(
                        terminal && source_end.is_some_and(|end| index > end),
                        "source owner dropped before exact provider/source end",
                    )?;
                }
            }
            Action::EndCallLoan(_) | Action::EndCaptureLoan { .. } => {
                require(
                    binding_end.is_some_and(|end| index < end),
                    "derived loan/capture cleanup must precede Map bindings",
                )?;
            }
            _ => {}
        }
    }
    if plan.source_access() == MapIterationSourceAccess::Temporary {
        let source_drops = drops
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(sites.source))
            .collect::<Vec<_>>();
        require(
            source_drops.len() == usize::from(terminal),
            "temporary has missing/duplicate same-edge cleanup",
        )?;
        for fact in source_drops {
            require(
                fact.value_origin() == sites.source_span
                    && fact.point() == point
                    && fact.condition().is_none()
                    && fact.capture_slot().is_none()
                    && fact.instance_address().is_none()
                    && matches!(fact.owner().and_then(|owner| owned.cleanup_conditions().owner_value(owner)),
                    Some(CleanupOwnerValue::Expression { expression, origin })
                        if *expression == sites.source && *origin == sites.source_span),
                "temporary cleanup lost actual factory owner/origin/point identity",
            )?;
        }
        let all = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(sites.source))
            .collect::<Vec<_>>();
        let [first, second] = all.as_slice() else {
            return Err("temporary must have separate terminal/exhaustion Drops");
        };
        require(
            first.owner().is_some() && first.owner() == second.owner(),
            "terminal/exhaustion do not share the actual temporary owner",
        )?;
    }
    Ok(())
}
