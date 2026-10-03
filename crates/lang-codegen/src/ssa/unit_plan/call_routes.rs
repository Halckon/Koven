//! 消费 frontend 已选择的静态 call route，形成具体 owner 参数与 callable identity。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::{DeclarationId, UnitSymbolId},
    ownership_checking::CompilationUnitOwnership,
    source::Span,
    type_checking::{
        CompilationUnitTypes, IntrinsicTypeConstructor, NominalKind, UnitCallableTarget,
        UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    ResolvedUnitCallInstance, UnitDelegatedCallRoute, UnitFunctionInstanceKey, UnitRecipeFailure,
    UnitRecipeRootFacts,
    concrete_types::resolve_direct_type_argument,
    contains_type_parameter, lowering_error,
    recipe_validation::{
        preflight_direct_inherited_owner_recipes, preflight_inherited_owner_recipes,
        preflight_owner_template_recipes, validate_dependent_inherited_nominal_recipe,
    },
    resolve_nominal_runtime_field_types, unit_callable_owner, unit_callable_signature,
};
use crate::ssa::{LoweringError, LoweringErrorKind};

fn dependent_inherited_owner_types(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: &[UnitTypeId],
    receiver: Option<UnitTypeId>,
    span: Span,
) -> Result<BTreeSet<UnitTypeId>, LoweringError> {
    let Some(receiver_declaration) = receiver.and_then(|receiver| {
        let UnitTypeKind::Nominal { declaration, .. } = typed.types().get(receiver)? else {
            return None;
        };
        Some(*declaration)
    }) else {
        return Ok(BTreeSet::new());
    };
    let Some((target_owner, owner_arity)) = unit_callable_owner(typed, target) else {
        return Ok(BTreeSet::new());
    };
    if target_owner == receiver_declaration || type_arguments.len() < owner_arity {
        return Ok(BTreeSet::new());
    }
    let mut roots = type_arguments
        .iter()
        .take(owner_arity)
        .filter_map(|&ty| {
            let UnitTypeKind::Nominal { declaration, .. } = typed.types().get(ty)? else {
                return None;
            };
            let nominal = typed.signatures().declaration(*declaration)?.nominal()?;
            Some((nominal.symbol(), *declaration, ty))
        })
        .collect::<Vec<_>>();
    // UnitSymbolId 携带 compilation index 规范化的 SourceUnitId 与源码内 symbol 顺序；
    // 以 concrete type 破同 declaration 的平局，避免 owner slot 顺序选择 witness。
    roots.sort_unstable();

    let mut dependent = BTreeSet::new();
    for (_, declaration, ty) in roots {
        let nominal = typed
            .signatures()
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal.fields().iter().any(|field| {
            typed
                .types()
                .get(field.ty())
                .is_some_and(|kind| contains_type_parameter(typed, kind))
        }) {
            validate_dependent_inherited_nominal_recipe(
                typed,
                nominal,
                span,
                &mut BTreeSet::new(),
            )?;
            dependent.insert(ty);
        }
    }
    Ok(dependent)
}

pub(crate) fn callable_static_self_receiver(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
) -> Result<bool, LoweringError> {
    let callable = unit_callable_signature(typed, target).ok_or(LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: None,
    })?;
    let Some(receiver) = callable.receiver() else {
        return Ok(false);
    };
    Ok(matches!(
        typed.types().get(receiver.ty()),
        Some(UnitTypeKind::StaticSelf(_))
    ))
}

/// 把 typed call target 与 concrete receiver 解析为 planner/lowerer 共用的实例 identity。
pub(crate) fn resolve_unit_call_instance(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
) -> Result<ResolvedUnitCallInstance, LoweringError> {
    resolve_unit_call_instance_with_recipe_failures(
        typed,
        owned,
        target,
        type_arguments,
        receiver,
        span,
        &UnitRecipeRootFacts::new(),
        &mut Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_unit_call_instance_with_recipe_failures(
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
    recipe_facts: &UnitRecipeRootFacts,
    recipe_failures: &mut Vec<UnitRecipeFailure>,
) -> Result<ResolvedUnitCallInstance, LoweringError> {
    macro_rules! direct {
        () => {
            resolve_direct_unit_call_instance(
                typed,
                target,
                type_arguments.clone(),
                receiver,
                span,
                recipe_failures,
            )
            .map(|key| ResolvedUnitCallInstance {
                key: key.0,
                delegation: Vec::new(),
                dependent_owner_types: key.1,
            })
        };
    }
    let Some(receiver) = receiver else {
        return direct!();
    };
    let mut current_receiver = receiver;
    let mut current_target = target;
    let mut current_type_arguments = type_arguments.clone();
    let mut delegation = Vec::new();
    let mut visited = BTreeSet::new();
    loop {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = typed.types().get(current_receiver)
        else {
            return if delegation.is_empty() {
                direct!()
            } else {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            };
        };
        let nominal = typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let routes = typed
            .signatures()
            .delegations()
            .iter()
            .filter(|plan| plan.owner() == *declaration)
            .flat_map(|plan| {
                plan.forwarders()
                    .iter()
                    .filter(move |forwarder| forwarder.requirement() == current_target)
                    .map(move |forwarder| (plan, forwarder))
            })
            .collect::<Vec<_>>();
        let (route, forwarder) = match routes.as_slice() {
            [] if delegation.is_empty() => return direct!(),
            [] => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            [(route, forwarder)] => (*route, *forwarder),
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if !visited.insert((*declaration, current_target)) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let ownership_routes = owned
            .delegations()
            .iter()
            .filter(|plan| {
                plan.owner() == *declaration
                    && plan.target() == route.target()
                    && plan.forwarders().contains(&current_target)
            })
            .collect::<Vec<_>>();
        if !matches!(ownership_routes.as_slice(), [_]) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if arguments.len() != nominal.type_parameters().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let concrete_fields =
            resolve_nominal_runtime_field_types(typed, current_receiver, nominal, arguments)?;
        let field_index = nominal
            .fields()
            .iter()
            .enumerate()
            .find(|(_, field)| field.symbol() == route.target())
            .map(|(index, _)| index)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let delegate_receiver = concrete_fields
            .get(field_index)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(UnitTypeKind::Nominal {
            declaration: delegate,
            arguments: delegate_arguments,
        }) = typed.types().get(delegate_receiver)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let delegate_nominal = typed
            .signatures()
            .declaration(*delegate)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if delegate_arguments.len() != delegate_nominal.type_parameters().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        resolve_nominal_runtime_field_types(
            typed,
            delegate_receiver,
            delegate_nominal,
            delegate_arguments,
        )?;
        delegation.push(UnitDelegatedCallRoute {
            outer_receiver: current_receiver,
            field: route.target(),
            delegate_receiver,
        });
        if let Some(next_hop) = forwarder.next_hop() {
            current_type_arguments = remap_delegation_next_hop_arguments(
                typed,
                current_target,
                next_hop.requirement(),
                forwarder.receiver_type(),
                next_hop.receiver_type(),
                nominal,
                arguments,
                &current_type_arguments,
                span,
            )?;
            current_target = next_hop.requirement();
            current_receiver = delegate_receiver;
            continue;
        }
        let Some(implementation) = forwarder.implementation() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };

        // Frontend 已选定 effective implementation；这里只校验 recipe 并重映射泛型槽位，
        // 不重新执行 member selection。
        let requirement_callable = unit_callable_signature(typed, current_target)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let implementation_callable = unit_callable_signature(typed, implementation.target())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (declared_implementation_owner, declared_implementation_owner_arity) =
            unit_callable_owner(typed, implementation.target())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if declared_implementation_owner != *delegate {
            let failures_before = recipe_failures.len();
            preflight_owner_template_recipes(
                typed,
                implementation.receiver_type(),
                recipe_facts,
                recipe_failures,
                span,
            )?;
            if recipe_failures.len() > failures_before {
                return Err(recipe_failures[failures_before].error);
            }
        }
        let (requirement_owner, requirement_owner_arguments) =
            instantiate_delegated_dispatch_owner_arguments(
                typed,
                forwarder.receiver_type(),
                nominal,
                arguments,
                span,
            )?;
        let (implementation_owner, implementation_owner_arguments) =
            instantiate_delegated_dispatch_owner_arguments(
                typed,
                implementation.receiver_type(),
                nominal,
                arguments,
                span,
            )?;
        let (declared_requirement_owner, declared_requirement_owner_arity) =
            unit_callable_owner(typed, current_target)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !implementation_callable.has_body()
            || implementation_callable.type_parameters().len()
                != requirement_callable.type_parameters().len()
            || declared_requirement_owner != requirement_owner
            || declared_implementation_owner != implementation_owner
            || declared_requirement_owner_arity != requirement_owner_arguments.len()
            || declared_implementation_owner_arity != implementation_owner_arguments.len()
            || current_type_arguments.len()
                != requirement_owner_arguments.len() + requirement_callable.type_parameters().len()
            || current_type_arguments[..requirement_owner_arguments.len()]
                != requirement_owner_arguments
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let callable_argument_start = requirement_owner_arguments.len();
        let mut implementation_arguments = implementation_owner_arguments;
        implementation_arguments
            .extend_from_slice(&current_type_arguments[callable_argument_start..]);
        let (key, dependent_owner_types) = resolve_direct_unit_call_instance(
            typed,
            implementation.target(),
            implementation_arguments,
            Some(delegate_receiver),
            span,
            recipe_failures,
        )?;
        return Ok(ResolvedUnitCallInstance {
            key,
            delegation,
            dependent_owner_types,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn remap_delegation_next_hop_arguments(
    typed: &CompilationUnitTypes,
    current_target: UnitCallableTarget,
    next_target: UnitCallableTarget,
    current_owner_template: UnitTypeId,
    next_owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    type_arguments: &[UnitTypeId],
    span: Span,
) -> Result<Vec<UnitTypeId>, LoweringError> {
    let current_callable = unit_callable_signature(typed, current_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let next_callable = unit_callable_signature(typed, next_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (current_owner, current_owner_arguments) = instantiate_dispatch_owner_arguments(
        typed,
        current_owner_template,
        concrete_owner,
        concrete_arguments,
        span,
    )?;
    let (next_owner, next_owner_arguments) = instantiate_dispatch_owner_arguments(
        typed,
        next_owner_template,
        concrete_owner,
        concrete_arguments,
        span,
    )?;
    let (declared_current_owner, current_owner_arity) = unit_callable_owner(typed, current_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (declared_next_owner, next_owner_arity) = unit_callable_owner(typed, next_target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if current_callable.type_parameters().len() != next_callable.type_parameters().len()
        || declared_current_owner != current_owner
        || declared_next_owner != next_owner
        || current_owner_arity != current_owner_arguments.len()
        || next_owner_arity != next_owner_arguments.len()
        || type_arguments.len()
            != current_owner_arguments.len() + current_callable.type_parameters().len()
        || type_arguments[..current_owner_arguments.len()] != current_owner_arguments
    {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let callable_argument_start = current_owner_arguments.len();
    let mut remapped = next_owner_arguments;
    remapped.extend_from_slice(&type_arguments[callable_argument_start..]);
    Ok(remapped)
}

fn resolve_direct_unit_call_instance(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
    span: Span,
    recipe_failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(UnitFunctionInstanceKey, BTreeSet<UnitTypeId>), LoweringError> {
    let callable = unit_callable_signature(typed, target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if callable.has_body() {
        let failures_before = recipe_failures.len();
        preflight_direct_inherited_owner_recipes(
            typed,
            target,
            &type_arguments,
            receiver,
            recipe_failures,
        )?;
        if recipe_failures.len() > failures_before {
            return Err(recipe_failures[failures_before].error);
        }
        let dependent_owner_types =
            dependent_inherited_owner_types(typed, target, &type_arguments, receiver, span)?;
        let static_self = if callable_static_self_receiver(typed, target)? {
            Some(receiver.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?)
        } else {
            None
        };
        return Ok((
            UnitFunctionInstanceKey::for_specialized_target(target, type_arguments, static_self),
            dependent_owner_types,
        ));
    }

    let receiver = receiver.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments: owner_arguments,
    }) = typed.types().get(receiver)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    let nominal = typed
        .signatures()
        .declaration(*declaration)
        .and_then(|signature| signature.nominal())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let dispatch = nominal
        .static_dispatch_overrides()
        .iter()
        .find(|dispatch| dispatch.requirement() == target)
        .copied()
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let implementation = dispatch.implementation();
    let implementation_callable = unit_callable_signature(typed, implementation)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (requirement_declaration, requirement_owner_arity) = unit_callable_owner(typed, target)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (implementation_declaration, implementation_owner_arity) =
        unit_callable_owner(typed, implementation)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let (requirement_owner, requirement_owner_arguments) = instantiate_dispatch_owner_arguments(
        typed,
        dispatch.requirement_owner(),
        nominal,
        owner_arguments,
        span,
    )?;
    let mut dependent_owner_types = BTreeSet::new();
    let (implementation_owner, implementation_owner_arguments) =
        if implementation_declaration == *declaration {
            instantiate_dispatch_owner_arguments(
                typed,
                dispatch.implementation_owner(),
                nominal,
                owner_arguments,
                span,
            )?
        } else {
            instantiate_inherited_dispatch_owner_arguments(
                typed,
                dispatch.implementation_owner(),
                nominal,
                owner_arguments,
                span,
                &mut dependent_owner_types,
                recipe_failures,
            )?
        };
    if !implementation_callable.has_body()
        || implementation_callable.type_parameters().len() != callable.type_parameters().len()
        || requirement_declaration != requirement_owner
        || implementation_declaration != implementation_owner
        || requirement_owner_arity != requirement_owner_arguments.len()
        || implementation_owner_arity != implementation_owner_arguments.len()
        || type_arguments.len()
            != requirement_owner_arguments.len() + callable.type_parameters().len()
        || type_arguments[..requirement_owner_arguments.len()] != requirement_owner_arguments
    {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let callable_argument_start = requirement_owner_arguments.len();
    let mut implementation_arguments = implementation_owner_arguments;
    implementation_arguments.extend_from_slice(&type_arguments[callable_argument_start..]);
    let static_self = callable_static_self_receiver(typed, implementation)?.then_some(receiver);
    Ok((
        UnitFunctionInstanceKey::for_specialized_target(
            implementation,
            implementation_arguments,
            static_self,
        ),
        dependent_owner_types,
    ))
}

fn instantiate_dispatch_owner_arguments(
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(owner_template)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    if concrete_owner.type_parameters().len() != concrete_arguments.len() {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let substitutions = concrete_owner
        .type_parameters()
        .iter()
        .copied()
        .zip(concrete_arguments.iter().copied())
        .collect::<BTreeMap<_, _>>();
    let arguments = arguments
        .iter()
        .map(|argument| resolve_direct_type_argument(typed, *argument, &substitutions, None, span))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((*declaration, arguments))
}

/// 只为 frontend 已选定的 inherited effective implementation 实例化有限 owner recipe。
fn instantiate_inherited_dispatch_owner_arguments(
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
    dependent_owner_types: &mut BTreeSet<UnitTypeId>,
    recipe_failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(owner_template)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    if concrete_owner.type_parameters().len() != concrete_arguments.len() {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let failures_before = recipe_failures.len();
    preflight_inherited_owner_recipes(typed, arguments, recipe_failures)?;
    if recipe_failures.len() > failures_before {
        return Err(recipe_failures[failures_before].error);
    }
    let substitutions = concrete_owner
        .type_parameters()
        .iter()
        .copied()
        .zip(concrete_arguments.iter().copied())
        .collect::<BTreeMap<_, _>>();
    let arguments = arguments
        .iter()
        .map(|argument| {
            resolve_inherited_dispatch_owner_argument(
                typed,
                *argument,
                &substitutions,
                span,
                &mut BTreeSet::new(),
                dependent_owner_types,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((*declaration, arguments))
}

fn instantiate_delegated_dispatch_owner_arguments(
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    concrete_owner: &UnitNominalSignature,
    concrete_arguments: &[UnitTypeId],
    span: Span,
) -> Result<(DeclarationId, Vec<UnitTypeId>), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(owner_template)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    if concrete_owner.type_parameters().len() != concrete_arguments.len() {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let substitutions = concrete_owner
        .type_parameters()
        .iter()
        .copied()
        .zip(concrete_arguments.iter().copied())
        .collect::<BTreeMap<_, _>>();
    let arguments = arguments
        .iter()
        .map(|argument| {
            resolve_delegated_dispatch_owner_argument(
                typed,
                *argument,
                &substitutions,
                span,
                &mut BTreeSet::new(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((*declaration, arguments))
}

pub(in crate::ssa) fn resolve_inherited_dispatch_owner_argument(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
    visiting: &mut BTreeSet<UnitTypeId>,
    dependent_owner_types: &mut BTreeSet<UnitTypeId>,
) -> Result<UnitTypeId, LoweringError> {
    if !visiting.insert(ty) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    match typed.types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        }) => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let argument = resolve_inherited_dispatch_owner_argument(
                typed,
                *argument,
                substitutions,
                span,
                visiting,
                dependent_owner_types,
            )?;
            typed
                .types()
                .find(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
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
            if nominal.kind() != NominalKind::Class
                || nominal.type_parameters().len() != 1
                || arguments.len() != 1
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            validate_dependent_inherited_nominal_recipe(
                typed,
                nominal,
                span,
                &mut BTreeSet::new(),
            )?;
            let argument = resolve_inherited_dispatch_owner_argument(
                typed,
                arguments[0],
                substitutions,
                span,
                visiting,
                dependent_owner_types,
            )?;
            let concrete = typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: *declaration,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if nominal.fields().iter().any(|field| {
                typed
                    .types()
                    .get(field.ty())
                    .is_some_and(|kind| contains_type_parameter(typed, kind))
            }) {
                dependent_owner_types.insert(concrete);
            }
            Ok(concrete)
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

/// 只在 frontend 已发布的 dispatch owner template 内递归替换现行 runtime recipe。
pub(in crate::ssa) fn resolve_delegated_dispatch_owner_argument(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
    visiting: &mut BTreeSet<UnitTypeId>,
) -> Result<UnitTypeId, LoweringError> {
    if !visiting.insert(ty) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    match typed.types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        }) => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let argument = resolve_delegated_dispatch_owner_argument(
                typed,
                *argument,
                substitutions,
                span,
                visiting,
            )?;
            typed
                .types()
                .find(&UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::List,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) if typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| {
                nominal.kind() == NominalKind::Class && nominal.type_parameters().len() == 1
            }) =>
        {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let argument = resolve_delegated_dispatch_owner_argument(
                typed,
                *argument,
                substitutions,
                span,
                visiting,
            )?;
            typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: *declaration,
                    arguments: vec![argument],
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}
