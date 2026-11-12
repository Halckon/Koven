//! 运行时类型需求升级与 exact owner 字段布局消费。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    parser::ParsedFile,
    source::Span,
    type_checking::{
        CompilationUnitTypes, IntrinsicTypeConstructor, NominalKind, UnitNominalSignature,
        UnitTypeId, UnitTypeKind,
    },
};

use super::{
    UnitPlannedInstance, UnitRuntimeTypeDemand, contains_type_parameter, lowering_error,
    resolve_concrete_type, span_contains, unit_callable_signature,
};
use crate::ssa::{LoweringError, LoweringErrorKind};

pub(super) fn classify_runtime_type_demands(
    typed: &CompilationUnitTypes,
    parsed_by_source: &[&ParsedFile],
    instances: &[UnitPlannedInstance],
    demands: &mut BTreeMap<UnitTypeId, UnitRuntimeTypeDemand>,
) -> Result<(), LoweringError> {
    if demands.is_empty() {
        return Ok(());
    }
    let dependent_types = demands.keys().copied().collect::<Vec<_>>();
    for instance in instances {
        let callable = if instance.key().deinit_owner().is_some() {
            None
        } else {
            Some(
                unit_callable_signature(typed, instance.key().target()).ok_or_else(|| {
                    lowering_error(LoweringErrorKind::MissingFact, instance.span())
                })?,
            )
        };
        for (template, span) in callable.into_iter().flat_map(|callable| {
            callable
                .parameters()
                .iter()
                .map(|parameter| (parameter.ty(), parameter.span()))
                .chain(std::iter::once((callable.return_type(), instance.span())))
        }) {
            let concrete = resolve_concrete_type(
                typed,
                template,
                instance.substitutions(),
                instance.key().static_self(),
                span,
            )?;
            upgrade_runtime_demands(typed, concrete, &dependent_types, demands, span)?;
        }

        let parsed = parsed_by_source
            .get(instance.source_unit().index())
            .copied()
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        for (&expression, &template) in typed.expression_types() {
            if expression.source_unit() != instance.source_unit() {
                continue;
            }
            let span = parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?
                .span();
            if !span_contains(instance.span(), span) {
                continue;
            }
            let concrete = resolve_concrete_type(
                typed,
                template,
                instance.substitutions(),
                instance.key().static_self(),
                span,
            )?;
            upgrade_runtime_demands(typed, concrete, &dependent_types, demands, span)?;
        }
    }

    for (&ty, &demand) in demands.iter() {
        if demand == UnitRuntimeTypeDemand::RuntimeLayoutRequired {
            require_exact_runtime_field_layout(typed, ty)?;
        }
    }
    Ok(())
}

fn upgrade_runtime_demands(
    typed: &CompilationUnitTypes,
    concrete: UnitTypeId,
    dependent_types: &[UnitTypeId],
    demands: &mut BTreeMap<UnitTypeId, UnitRuntimeTypeDemand>,
    span: Span,
) -> Result<(), LoweringError> {
    for &dependent in dependent_types {
        if runtime_storage_depends_on(typed, concrete, dependent, &mut BTreeSet::new(), span)? {
            demands.insert(dependent, UnitRuntimeTypeDemand::RuntimeLayoutRequired);
        }
    }
    Ok(())
}

fn runtime_storage_depends_on(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    dependent: UnitTypeId,
    visiting: &mut BTreeSet<UnitTypeId>,
    span: Span,
) -> Result<bool, LoweringError> {
    if ty == dependent {
        return Ok(true);
    }
    if !visiting.insert(ty) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    let result = match typed.types().get(ty) {
        Some(UnitTypeKind::Nullable(inner)) | Some(UnitTypeKind::StaticSelf(inner)) => {
            runtime_storage_depends_on(typed, *inner, dependent, visiting, span)
        }
        Some(UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        }) => parameters
            .iter()
            .map(|parameter| parameter.ty())
            .chain(std::iter::once(*return_type))
            .try_fold(false, |found, nested| {
                Ok(found || runtime_storage_depends_on(typed, nested, dependent, visiting, span)?)
            }),
        Some(UnitTypeKind::Intrinsic { arguments, .. }) => {
            arguments
                .iter()
                .copied()
                .try_fold(false, |found, argument| {
                    Ok(found
                        || runtime_storage_depends_on(typed, argument, dependent, visiting, span)?)
                })
        }
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) => {
            let nominal = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let fields = if nominal.kind() == NominalKind::EnumClass {
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments.iter().copied())
                    .collect::<BTreeMap<_, _>>();
                nominal
                    .enum_cases()
                    .iter()
                    .flat_map(|case| case.payloads())
                    .map(|payload| {
                        resolve_concrete_type(
                            typed,
                            payload.ty(),
                            &substitutions,
                            None,
                            payload.span(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)?
            };
            fields.into_iter().try_fold(false, |found, field| {
                Ok(found || runtime_storage_depends_on(typed, field, dependent, visiting, span)?)
            })
        }
        Some(UnitTypeKind::EnumCase { root, .. }) => {
            runtime_storage_depends_on(typed, *root, dependent, visiting, span)
        }
        Some(_) => Ok(false),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }?;
    visiting.remove(&ty);
    Ok(result)
}

fn require_exact_runtime_field_layout(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
) -> Result<(), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(ty)
    else {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        });
    };
    let nominal = typed
        .signatures()
        .declaration(*declaration)
        .and_then(|signature| signature.nominal())
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    if typed.runtime_field_layout(ty).is_none() {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: nominal.fields().first().map(|field| field.span()),
        });
    }
    resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)?;
    Ok(())
}

/// 把 nominal runtime fields 解析为当前 concrete owner instance 的存储类型。
///
/// 优先消费 SPEC-0219 按 exact owner 发布的 descriptor。字段替换中新 intern、未进入 frontend
/// owner 候选快照的 nested owner 只允许回退到 closed/direct type-parameter 旧规则。
pub(crate) fn resolve_nominal_runtime_field_types(
    typed: &CompilationUnitTypes,
    owner_type: UnitTypeId,
    nominal: &UnitNominalSignature,
    arguments: &[UnitTypeId],
) -> Result<Vec<UnitTypeId>, LoweringError> {
    let owner_matches = matches!(
        typed.types().get(owner_type),
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments: owner_arguments,
        }) if *declaration == nominal.declaration() && owner_arguments == arguments
    );
    if !owner_matches {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        });
    }
    if nominal.type_parameters().len() != arguments.len() {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        });
    }
    if let Some(layout) = typed.runtime_field_layout(owner_type) {
        if layout.declaration() != nominal.declaration()
            || layout.arguments() != arguments
            || layout.fields().len() != nominal.fields().len()
        {
            return Err(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            });
        }
        return layout
            .fields()
            .iter()
            .zip(nominal.fields())
            .map(|(actual, template)| {
                if actual.symbol() != template.symbol()
                    || actual.template_type() != template.ty()
                    || actual.span() != template.span()
                    || typed.types().get(actual.concrete_type()).is_none()
                {
                    Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        template.span(),
                    ))
                } else if !supported_nested_runtime_field_recipe(typed, nominal, template.ty()) {
                    Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        template.span(),
                    ))
                } else {
                    Ok(actual.concrete_type())
                }
            })
            .collect();
    }
    nominal
        .fields()
        .iter()
        .map(|field| match typed.types().get(field.ty()) {
            Some(UnitTypeKind::TypeParameter(parameter)) => nominal
                .type_parameters()
                .iter()
                .position(|candidate| candidate == parameter)
                .and_then(|index| arguments.get(index).copied())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, field.span())),
            Some(kind) if contains_type_parameter(typed, kind) => Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                field.span(),
            )),
            Some(_) => Ok(field.ty()),
            None => Err(lowering_error(LoweringErrorKind::MissingFact, field.span())),
        })
        .collect()
}

/// SPEC-0191 的递归 recipe 仍限 `List`、单参数 ordinary class 与 direct owner parameter。
/// 顶层 nullable 另接受 direct owner parameter，或非泛型 ordinary class 中的非泛型 class；
/// 其余 nullable recipe 不因类型闭合而自动放行，递归 owner 仍由 type mapper 拒绝。
fn supported_nested_runtime_field_recipe(
    typed: &CompilationUnitTypes,
    owner: &UnitNominalSignature,
    template: UnitTypeId,
) -> bool {
    if let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(template) {
        if direct_owner_type_parameter(typed, owner, *inner) {
            return true;
        }
        if owner.kind() != NominalKind::Class || !owner.type_parameters().is_empty() {
            return false;
        }
        return matches!(
            typed.types().get(*inner),
            Some(UnitTypeKind::Nominal { declaration, arguments })
                if arguments.is_empty()
                    && typed
                        .signatures()
                        .declaration(*declaration)
                        .and_then(|signature| signature.nominal())
                        .is_some_and(|nested| {
                            nested.kind() == NominalKind::Class
                                && nested.type_parameters().is_empty()
                        })
        );
    }
    supported_nested_runtime_field_recipe_with(typed, owner, template, &mut BTreeSet::new())
}

fn supported_nested_runtime_field_recipe_with(
    typed: &CompilationUnitTypes,
    owner: &UnitNominalSignature,
    template: UnitTypeId,
    visiting: &mut BTreeSet<UnitTypeId>,
) -> bool {
    if !visiting.insert(template) {
        return false;
    }
    let Some(kind) = typed.types().get(template) else {
        return false;
    };
    if !contains_type_parameter(typed, kind) {
        return true;
    }
    match kind {
        UnitTypeKind::TypeParameter(parameter) => owner.type_parameters().contains(parameter),
        UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        } => matches!(
            arguments.as_slice(),
            [argument]
                if supported_nested_runtime_field_recipe_with(
                    typed,
                    owner,
                    *argument,
                    visiting,
                )
        ),
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            matches!(
                arguments.as_slice(),
                [argument]
                    if supported_nested_runtime_field_recipe_with(
                        typed,
                        owner,
                        *argument,
                        visiting,
                    )
            ) && typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nested| {
                    nested.kind() == NominalKind::Class && nested.type_parameters().len() == 1
                })
        }
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Nullable(_)
        | UnitTypeKind::Function { .. }
        | UnitTypeKind::Intrinsic { .. }
        | UnitTypeKind::EnumCase { .. }
        | UnitTypeKind::StaticSelf(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => false,
    }
}

fn direct_owner_type_parameter(
    typed: &CompilationUnitTypes,
    owner: &UnitNominalSignature,
    ty: UnitTypeId,
) -> bool {
    matches!(
        typed.types().get(ty),
        Some(UnitTypeKind::TypeParameter(parameter))
            if owner.type_parameters().contains(parameter)
    )
}

pub(in crate::ssa::unit_plan) mod recipe_preflight {
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

    use super::super::{
        ResolvedUnitCallInstance, UnitFunctionInstanceKey, UnitFunctionTemplate,
        UnitPlannedInstance, UnitRecipeFailure, UnitRecipeRootFacts,
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
    pub(in crate::ssa::unit_plan) fn pending_recipe_frontier(
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

    pub(in crate::ssa::unit_plan) fn preflight_unit_call_recipes(
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
            UnitCallTarget::Declaration(declaration) => {
                UnitCallableTarget::Declaration(declaration)
            }
            UnitCallTarget::Symbol(symbol) => UnitCallableTarget::Symbol(symbol),
            UnitCallTarget::External(_) | UnitCallTarget::FunctionValue => return Ok(None),
        };
        let specialized_type_arguments = call
            .instance()
            .type_arguments()
            .iter()
            .map(|&ty| specialize_preflight_type(typed, ty, static_self, substitutions))
            .collect::<Option<Vec<_>>>();
        let specialized_receiver = match call.receiver() {
            Some(receiver) => {
                specialize_preflight_type(typed, receiver.ty(), static_self, substitutions)
                    .map(Some)
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
            let Some((implementation_owner, _)) =
                unit_callable_owner(typed, implementation.target())
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

    pub(in crate::ssa::unit_plan) fn collect_frontier_unit_recipe_failures(
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

                if owned.call_may_dispatch(typed.calls()[*call_index].expression()) == Some(false) {
                    continue;
                }
                let Some(&target_template) = template_by_target.get(&resolved.key().target())
                else {
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

    pub(in crate::ssa::unit_plan) fn stable_recipe_failure(
        failures: &mut [UnitRecipeFailure],
    ) -> LoweringError {
        failures.sort_unstable_by_key(|failure| failure.root);
        failures[0].error
    }
}
