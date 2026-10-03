//! 沿有限 callable frontier 传播 recipe facts，并保持失败 witness 与实例上限的优先级。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::CompilationUnitOwnership,
    source::Span,
    type_checking::{
        CompilationUnitTypes, UnitCallTarget, UnitCallableTarget, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    ResolvedUnitCallInstance, UnitFunctionInstanceKey, UnitFunctionTemplate, UnitPlannedInstance,
    UnitRecipeFailure, UnitRecipeRootFacts,
    call_routes::resolve_unit_call_instance_with_recipe_failures,
    concrete_types::specialize_preflight_type,
    contains_type_parameter, lowering_error,
    recipe_validation::{
        extend_recipe_root_facts, preflight_direct_inherited_owner_recipes_with_facts,
        preflight_owner_template_recipes, recipe_root_declarations_in_types,
        recipe_root_facts_for_instance,
    },
    unit_callable_owner,
};
use crate::ssa::{LoweringError, LoweringErrorKind};

/// 在当前 concrete key 命中实例上限时，只沿可达 callable template 图收集 recipe failure。
///
/// 图节点只包含有限的 callable identity 与 frontend 已发布的 `StaticSelf` type identity，
/// 不创建变化后的 type arguments，因此不会被 generic instance 上限或 pending key 顺序截断。
/// 普通 lowering 错误仍由正式 planner 的 concrete frontier 报告。
pub(super) fn pending_recipe_frontier(
    current: &UnitFunctionInstanceKey,
    pending: &BTreeSet<UnitFunctionInstanceKey>,
    planned: &BTreeMap<UnitFunctionInstanceKey, UnitPlannedInstance>,
) -> Vec<UnitFunctionInstanceKey> {
    std::iter::once(current.clone())
        .chain(
            pending
                .iter()
                .filter(|candidate| !planned.contains_key(*candidate))
                .cloned(),
        )
        .collect()
}

pub(super) fn preflight_unit_call_recipes(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    call_index: usize,
    span: Span,
    context: (
        Option<UnitTypeId>,
        Option<&BTreeMap<UnitSymbolId, UnitTypeId>>,
        &UnitRecipeRootFacts,
    ),
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<Option<ResolvedUnitCallInstance>, LoweringError> {
    let (static_self, substitutions, facts) = context;
    let call = &typed.calls()[call_index];
    let target = match call.target() {
        UnitCallTarget::Declaration(declaration) => UnitCallableTarget::Declaration(declaration),
        UnitCallTarget::Symbol(symbol) => UnitCallableTarget::Symbol(symbol),
        UnitCallTarget::External(_)
        | UnitCallTarget::FunctionValue
        | UnitCallTarget::StructuralComponent(_) => return Ok(None),
    };
    let specialized_type_arguments = call
        .instance()
        .type_arguments()
        .iter()
        .map(|&ty| specialize_preflight_type(typed, ty, static_self, substitutions))
        .collect::<Option<Vec<_>>>();
    let specialized_receiver = match call.receiver() {
        Some(receiver) => {
            specialize_preflight_type(typed, receiver.ty(), static_self, substitutions).map(Some)
        }
        None => Some(None),
    };
    if let (Some(type_arguments), Some(receiver)) =
        (specialized_type_arguments.as_ref(), specialized_receiver)
    {
        let failures_before = failures.len();
        let has_unresolved_type = type_arguments.iter().copied().chain(receiver).any(|ty| {
            typed
                .types()
                .get(ty)
                .is_none_or(|kind| contains_type_parameter(typed, kind))
        });
        let resolved = resolve_unit_call_instance_with_recipe_failures(
            typed,
            owned,
            target,
            type_arguments.clone(),
            receiver,
            span,
            facts,
            failures,
        );
        match resolved {
            Ok(resolved) => {
                let effective_receiver = resolved
                    .key()
                    .static_self()
                    .or_else(|| {
                        resolved
                            .delegation()
                            .last()
                            .map(|route| route.delegate_receiver())
                    })
                    .or(receiver);
                if preflight_direct_inherited_owner_recipes_with_facts(
                    typed,
                    resolved.key().target(),
                    resolved.key().type_arguments(),
                    effective_receiver,
                    facts,
                    failures,
                )
                .is_err()
                {
                    return Ok(None);
                }
                if failures.len() > failures_before {
                    return Ok(None);
                }
                return Ok(Some(resolved));
            }
            Err(_) if failures.len() > failures_before || !has_unresolved_type => {
                return Ok(None);
            }
            Err(_) => {}
        }
    }

    let preflight_type_arguments = specialized_type_arguments
        .as_deref()
        .unwrap_or_else(|| call.instance().type_arguments());
    let preflight_receiver = specialized_receiver
        .flatten()
        .or_else(|| call.receiver().map(|receiver| receiver.ty()));
    let delegation_preflight = preflight_delegation_endpoint_owner_recipes(
        typed,
        target,
        preflight_receiver,
        facts,
        failures,
        span,
    );
    let Ok(has_delegation_route) = delegation_preflight else {
        return Ok(None);
    };
    if !has_delegation_route
        && preflight_direct_inherited_owner_recipes_with_facts(
            typed,
            target,
            preflight_type_arguments,
            preflight_receiver,
            facts,
            failures,
        )
        .is_err()
    {
        return Ok(None);
    }
    Ok(None)
}

/// 只消费 frontend 已选定的 delegation endpoint；local override 不需要 inherited owner recipe。
fn preflight_delegation_endpoint_owner_recipes(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    receiver: Option<UnitTypeId>,
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
    span: Span,
) -> Result<bool, LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration: receiver_declaration,
        arguments: receiver_arguments,
    }) = receiver.and_then(|receiver| typed.types().get(receiver))
    else {
        return Ok(false);
    };
    let receiver_nominal = typed
        .signatures()
        .declaration(*receiver_declaration)
        .and_then(|signature| signature.nominal())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let mut current_owner = *receiver_declaration;
    let mut current_target = target;
    let mut current_facts = extend_recipe_root_facts(
        typed,
        receiver_nominal.type_parameters(),
        receiver_arguments,
        facts,
    )?;
    let mut visited = BTreeSet::new();
    let mut found_route = false;

    loop {
        if !visited.insert((current_owner, current_target)) {
            return Ok(found_route);
        }
        let routes = typed
            .signatures()
            .delegations()
            .iter()
            .filter(|plan| plan.owner() == current_owner)
            .flat_map(|plan| {
                plan.forwarders()
                    .iter()
                    .filter(move |forwarder| forwarder.requirement() == current_target)
                    .map(move |forwarder| (plan, forwarder))
            })
            .collect::<Vec<_>>();
        let [(route, forwarder)] = routes.as_slice() else {
            return Ok(found_route || !routes.is_empty());
        };
        found_route = true;
        let Some((delegate_declaration, delegate_arguments)) = typed
            .signatures()
            .declaration(current_owner)
            .and_then(|signature| signature.nominal())
            .and_then(|nominal| {
                nominal
                    .fields()
                    .iter()
                    .find(|field| field.symbol() == route.target())
            })
            .and_then(|field| match typed.types().get(field.ty()) {
                Some(UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                }) => Some((*declaration, arguments.as_slice())),
                _ => None,
            })
        else {
            return Ok(true);
        };
        let delegate_nominal = typed
            .signatures()
            .declaration(delegate_declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let delegate_facts = extend_recipe_root_facts(
            typed,
            delegate_nominal.type_parameters(),
            delegate_arguments,
            &current_facts,
        )?;
        if let Some(next_hop) = forwarder.next_hop() {
            current_owner = delegate_declaration;
            current_target = next_hop.requirement();
            current_facts = delegate_facts;
            continue;
        }
        let Some(implementation) = forwarder.implementation() else {
            return Ok(true);
        };
        let Some((implementation_owner, _)) = unit_callable_owner(typed, implementation.target())
        else {
            return Ok(true);
        };
        if implementation_owner != delegate_declaration {
            preflight_owner_template_recipes(
                typed,
                implementation.receiver_type(),
                &delegate_facts,
                failures,
                span,
            )?;
        }
        return Ok(true);
    }
}

pub(super) fn collect_frontier_unit_recipe_failures(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    templates: &[UnitFunctionTemplate],
    template_by_target: &BTreeMap<UnitCallableTarget, usize>,
    calls_by_template: &[Vec<(usize, Span)>],
    frontier: &[UnitFunctionInstanceKey],
) -> Result<Vec<UnitRecipeFailure>, LoweringError> {
    let mut failures = Vec::new();
    for candidate in frontier {
        let Some(&template_index) = template_by_target.get(&candidate.target()) else {
            continue;
        };
        let template = &templates[template_index];
        if template.type_parameters.len() != candidate.type_arguments().len() {
            continue;
        }
        let facts = recipe_root_facts_for_instance(
            typed,
            &template.type_parameters,
            candidate.type_arguments(),
        )?;
        let substitutions = template
            .type_parameters
            .iter()
            .copied()
            .zip(candidate.type_arguments().iter().copied())
            .collect::<BTreeMap<_, _>>();
        for (call_index, span) in &calls_by_template[template_index] {
            preflight_unit_call_recipes(
                typed,
                owned,
                *call_index,
                *span,
                (candidate.static_self(), Some(&substitutions), &facts),
                &mut failures,
            )?;
        }
        failures.extend(collect_reachable_unit_recipe_failures(
            typed,
            owned,
            templates,
            template_by_target,
            calls_by_template,
            (template_index, candidate.static_self(), facts),
        )?);
    }
    Ok(failures)
}

fn collect_reachable_unit_recipe_failures(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    templates: &[UnitFunctionTemplate],
    template_by_target: &BTreeMap<UnitCallableTarget, usize>,
    calls_by_template: &[Vec<(usize, Span)>],
    entry: (usize, Option<UnitTypeId>, UnitRecipeRootFacts),
) -> Result<Vec<UnitRecipeFailure>, LoweringError> {
    let (entry_template, entry_static_self, entry_facts) = entry;
    let entry_context = (entry_template, entry_static_self);
    let mut pending = BTreeSet::from([entry_context]);
    let mut facts_by_context = BTreeMap::from([(entry_context, entry_facts)]);
    let mut failures = Vec::new();

    while let Some((template_index, static_self)) = pending.pop_first() {
        let facts = facts_by_context
            .get(&(template_index, static_self))
            .cloned()
            .unwrap_or_default();
        for (call_index, span) in &calls_by_template[template_index] {
            let Some(resolved) = preflight_unit_call_recipes(
                typed,
                owned,
                *call_index,
                *span,
                (static_self, None, &facts),
                &mut failures,
            )?
            else {
                continue;
            };

            let Some(&target_template) = template_by_target.get(&resolved.key().target()) else {
                continue;
            };
            if templates[target_template].type_parameters.len()
                != resolved.key().type_arguments().len()
            {
                continue;
            }
            let next_roots = templates[target_template]
                .type_parameters
                .iter()
                .zip(resolved.key().type_arguments())
                .map(|(&parameter, &argument)| {
                    recipe_root_declarations_in_types(typed, &[argument], &facts)
                        .map(|roots| (parameter, roots))
                })
                .collect::<Result<Vec<_>, _>>();
            let Ok(next_roots) = next_roots else {
                continue;
            };
            let mut next_facts = facts.clone();
            for (parameter, roots) in next_roots {
                next_facts.entry(parameter).or_default().extend(roots);
            }
            let context = (target_template, resolved.key().static_self());
            let is_new = !facts_by_context.contains_key(&context);
            let stored = facts_by_context.entry(context).or_default();
            let mut changed = false;
            for (parameter, roots) in next_facts {
                let current = stored.entry(parameter).or_default();
                let previous = current.len();
                current.extend(roots);
                changed |= current.len() != previous;
            }
            if is_new || changed {
                pending.insert(context);
            }
        }
    }

    Ok(failures)
}

pub(super) fn stable_recipe_failure(failures: &mut [UnitRecipeFailure]) -> LoweringError {
    failures.sort_unstable_by_key(|failure| failure.root);
    failures[0].error
}
