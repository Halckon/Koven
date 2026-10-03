//! owner recipe root facts、有限 declaration graph 许可与 closed recipe 回边校验。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::{DeclarationId, UnitSymbolId},
    source::Span,
    type_checking::{
        CompilationUnitTypes, IntrinsicTypeConstructor, NominalKind, UnitCallableTarget,
        UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    UnitRecipeFailure, UnitRecipeRootFacts, contains_type_parameter, lowering_error,
    resolve_nominal_runtime_field_types, unit_callable_owner,
};
use crate::ssa::{LoweringError, LoweringErrorKind};

pub(super) fn recipe_root_facts_for_instance(
    typed: &CompilationUnitTypes,
    parameters: &[UnitSymbolId],
    arguments: &[UnitTypeId],
) -> Result<UnitRecipeRootFacts, LoweringError> {
    let empty = UnitRecipeRootFacts::new();
    extend_recipe_root_facts(typed, parameters, arguments, &empty)
}

pub(super) fn extend_recipe_root_facts(
    typed: &CompilationUnitTypes,
    parameters: &[UnitSymbolId],
    arguments: &[UnitTypeId],
    facts: &UnitRecipeRootFacts,
) -> Result<UnitRecipeRootFacts, LoweringError> {
    if parameters.len() != arguments.len() {
        return Err(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        });
    }
    let additions = parameters
        .iter()
        .copied()
        .zip(arguments.iter().copied())
        .map(|(parameter, argument)| {
            recipe_root_declarations_in_types(typed, &[argument], facts)
                .map(|roots| (parameter, roots))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut extended = facts.clone();
    for (parameter, roots) in additions {
        extended.entry(parameter).or_default().extend(roots);
    }
    Ok(extended)
}

pub(super) fn preflight_owner_template_recipes(
    typed: &CompilationUnitTypes,
    owner_template: UnitTypeId,
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
    span: Span,
) -> Result<(), LoweringError> {
    let Some(UnitTypeKind::Nominal { arguments, .. }) = typed.types().get(owner_template) else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    preflight_inherited_owner_recipes_with_facts(typed, arguments, facts, failures)
}

pub(super) fn preflight_direct_inherited_owner_recipes(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: &[UnitTypeId],
    receiver: Option<UnitTypeId>,
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    preflight_direct_inherited_owner_recipes_with_facts(
        typed,
        target,
        type_arguments,
        receiver,
        &UnitRecipeRootFacts::new(),
        failures,
    )
}

pub(super) fn preflight_direct_inherited_owner_recipes_with_facts(
    typed: &CompilationUnitTypes,
    target: UnitCallableTarget,
    type_arguments: &[UnitTypeId],
    receiver: Option<UnitTypeId>,
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration: receiver_declaration,
        ..
    }) = receiver.and_then(|receiver| typed.types().get(receiver))
    else {
        return Ok(());
    };
    let Some((target_owner, owner_arity)) = unit_callable_owner(typed, target) else {
        return Ok(());
    };
    if target_owner == *receiver_declaration || type_arguments.len() < owner_arity {
        return Ok(());
    }
    preflight_inherited_owner_recipes_with_facts(
        typed,
        &type_arguments[..owner_arity],
        facts,
        failures,
    )
}

/// 在按 owner slot 实例化前，以稳定 source identity 选择 dependent recipe 的失败 witness。
pub(super) fn preflight_inherited_owner_recipes(
    typed: &CompilationUnitTypes,
    arguments: &[UnitTypeId],
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    preflight_inherited_owner_recipes_with_facts(
        typed,
        arguments,
        &UnitRecipeRootFacts::new(),
        failures,
    )
}

pub(super) fn recipe_root_declarations_in_types(
    typed: &CompilationUnitTypes,
    arguments: &[UnitTypeId],
    facts: &UnitRecipeRootFacts,
) -> Result<BTreeSet<DeclarationId>, LoweringError> {
    fn collect(
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        facts: &UnitRecipeRootFacts,
        roots: &mut BTreeSet<DeclarationId>,
    ) -> Result<(), LoweringError> {
        match typed.types().get(ty) {
            Some(UnitTypeKind::TypeParameter(parameter)) => {
                if let Some(substituted) = facts.get(parameter) {
                    roots.extend(substituted);
                }
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                let nominal = typed
                    .signatures()
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .ok_or(LoweringError {
                        kind: LoweringErrorKind::MissingFact,
                        span: None,
                    })?;
                if nominal.kind() == NominalKind::Class && nominal.type_parameters().len() == 1 {
                    roots.insert(nominal.declaration());
                    if let [argument] = arguments.as_slice() {
                        collect(typed, *argument, facts, roots)?;
                    }
                }
            }
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            }) => {
                for &argument in arguments {
                    collect(typed, argument, facts, roots)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    let mut roots = BTreeSet::new();
    for &argument in arguments {
        collect(typed, argument, facts, &mut roots)?;
    }
    Ok(roots)
}

fn preflight_inherited_owner_recipes_with_facts(
    typed: &CompilationUnitTypes,
    arguments: &[UnitTypeId],
    facts: &UnitRecipeRootFacts,
    failures: &mut Vec<UnitRecipeFailure>,
) -> Result<(), LoweringError> {
    let declarations = recipe_root_declarations_in_types(typed, arguments, facts)?;
    let mut roots = declarations
        .into_iter()
        .map(|declaration| {
            typed
                .signatures()
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    roots.sort_unstable_by_key(|nominal| nominal.symbol());
    for nominal in roots {
        if let RecipeTraversal::Cycle(witness) =
            parameter_growing_recipe_witness(typed, nominal, &mut BTreeSet::new())?
        {
            failures.push(UnitRecipeFailure {
                root: nominal.symbol(),
                error: lowering_error(LoweringErrorKind::UnsupportedNode, witness),
            });
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecipeTraversal {
    Clean,
    Cycle(Span),
    Unsupported,
}

/// 只识别 ADR-0024 的 declaration back-edge；unsupported constructor 仍交给正式 resolver。
fn parameter_growing_recipe_witness(
    typed: &CompilationUnitTypes,
    nominal: &UnitNominalSignature,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<RecipeTraversal, LoweringError> {
    if nominal.kind() != NominalKind::Class || nominal.type_parameters().len() != 1 {
        return Ok(RecipeTraversal::Unsupported);
    }
    if !visiting.insert(nominal.declaration()) {
        return Ok(nominal
            .fields()
            .first()
            .map_or(RecipeTraversal::Clean, |field| {
                RecipeTraversal::Cycle(field.span())
            }));
    }

    fn in_type(
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        owner_parameter: UnitSymbolId,
        span: Span,
        visiting: &mut BTreeSet<DeclarationId>,
    ) -> Result<RecipeTraversal, LoweringError> {
        let kind = typed
            .types()
            .get(ty)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !contains_type_parameter(typed, kind) {
            return parameter_growing_closed_recipe_witness(typed, ty, span, visiting);
        }
        match kind {
            UnitTypeKind::TypeParameter(parameter) if *parameter == owner_parameter => {
                Ok(RecipeTraversal::Clean)
            }
            UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            } => {
                let [argument] = arguments.as_slice() else {
                    return Ok(RecipeTraversal::Unsupported);
                };
                in_type(typed, *argument, owner_parameter, span, visiting)
            }
            UnitTypeKind::Nominal {
                declaration,
                arguments,
            } => {
                let [argument] = arguments.as_slice() else {
                    return Ok(RecipeTraversal::Unsupported);
                };
                if visiting.contains(declaration) {
                    return Ok(RecipeTraversal::Cycle(span));
                }
                let nested = typed
                    .signatures()
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let result = parameter_growing_recipe_witness(typed, nested, visiting)?;
                if result != RecipeTraversal::Clean {
                    return Ok(result);
                }
                if matches!(
                    typed.types().get(*argument),
                    Some(UnitTypeKind::TypeParameter(parameter)) if *parameter == owner_parameter
                ) {
                    Ok(RecipeTraversal::Clean)
                } else {
                    Ok(RecipeTraversal::Unsupported)
                }
            }
            _ => Ok(RecipeTraversal::Unsupported),
        }
    }

    let owner_parameter = nominal.type_parameters()[0];
    let mut result = RecipeTraversal::Clean;
    for field in nominal.fields() {
        result = in_type(typed, field.ty(), owner_parameter, field.span(), visiting)?;
        if result != RecipeTraversal::Clean {
            break;
        }
    }
    visiting.remove(&nominal.declaration());
    Ok(result)
}

fn parameter_growing_closed_recipe_witness(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<RecipeTraversal, LoweringError> {
    let kind = typed
        .types()
        .get(ty)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match kind {
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            if !visiting.insert(*declaration) {
                return Ok(RecipeTraversal::Cycle(span));
            }
            let nominal = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let concrete_fields = if nominal.kind() != NominalKind::EnumClass
                && typed.runtime_field_layout(ty).is_some()
            {
                match resolve_nominal_runtime_field_types(typed, ty, nominal, arguments) {
                    Ok(fields) => fields
                        .into_iter()
                        .zip(nominal.fields())
                        .map(|(concrete, field)| (concrete, field.span()))
                        .collect::<Vec<_>>(),
                    Err(error) if error.kind == LoweringErrorKind::UnsupportedNode => {
                        visiting.remove(declaration);
                        return Ok(RecipeTraversal::Unsupported);
                    }
                    Err(error) => {
                        visiting.remove(declaration);
                        return Err(error);
                    }
                }
            } else {
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments.iter().copied())
                    .collect::<BTreeMap<_, _>>();
                let mut concrete = Vec::new();
                for (template, field_span) in nominal
                    .fields()
                    .iter()
                    .map(|field| (field.ty(), field.span()))
                    .chain(nominal.enum_cases().iter().flat_map(|case| {
                        case.payloads()
                            .iter()
                            .map(|payload| (payload.ty(), payload.span()))
                    }))
                {
                    match resolve_closed_recipe_type(typed, template, &substitutions, field_span) {
                        Ok(ty) => concrete.push((ty, field_span)),
                        Err(error) if error.kind == LoweringErrorKind::UnsupportedNode => {
                            visiting.remove(declaration);
                            return Ok(RecipeTraversal::Unsupported);
                        }
                        Err(error) => {
                            visiting.remove(declaration);
                            return Err(error);
                        }
                    }
                }
                concrete
            };
            let mut result = RecipeTraversal::Clean;
            for (concrete, field_span) in concrete_fields {
                result =
                    parameter_growing_closed_recipe_witness(typed, concrete, field_span, visiting)?;
                if result != RecipeTraversal::Clean {
                    break;
                }
            }
            visiting.remove(declaration);
            Ok(result)
        }
        UnitTypeKind::Intrinsic { arguments, .. } => {
            for &argument in arguments {
                let result =
                    parameter_growing_closed_recipe_witness(typed, argument, span, visiting)?;
                if result != RecipeTraversal::Clean {
                    return Ok(result);
                }
            }
            Ok(RecipeTraversal::Clean)
        }
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => {
            parameter_growing_closed_recipe_witness(typed, *inner, span, visiting)
        }
        UnitTypeKind::EnumCase { root, .. } => {
            parameter_growing_closed_recipe_witness(typed, *root, span, visiting)
        }
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Function { .. }
        | UnitTypeKind::TypeParameter(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => Ok(RecipeTraversal::Clean),
    }
}

/// dependent inherited owner 只开放有限、非增长的单参数 ordinary-class field graph。
pub(super) fn validate_dependent_inherited_nominal_recipe(
    typed: &CompilationUnitTypes,
    nominal: &UnitNominalSignature,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<(), LoweringError> {
    if nominal.kind() != NominalKind::Class || nominal.type_parameters().len() != 1 {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    if !visiting.insert(nominal.declaration()) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    }
    let parameter = nominal.type_parameters()[0];
    for field in nominal.fields() {
        validate_dependent_inherited_field_recipe(
            typed,
            field.ty(),
            parameter,
            field.span(),
            visiting,
        )?;
    }
    visiting.remove(&nominal.declaration());
    Ok(())
}

fn validate_dependent_inherited_field_recipe(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    owner_parameter: UnitSymbolId,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<(), LoweringError> {
    let kind = typed
        .types()
        .get(ty)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if !contains_type_parameter(typed, kind) {
        return validate_closed_nominal_recipe_cycles(typed, ty, span, visiting);
    }
    match kind {
        UnitTypeKind::TypeParameter(parameter) if *parameter == owner_parameter => Ok(()),
        UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        } => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            validate_dependent_inherited_field_recipe(
                typed,
                *argument,
                owner_parameter,
                span,
                visiting,
            )
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            let nested = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            // ADR-0024：先沿有限 declaration/template graph 找第一条回边；不能因
            // concrete argument 形状复杂而在更早的边上产生不稳定 witness。
            validate_dependent_inherited_nominal_recipe(typed, nested, span, visiting)?;
            if !matches!(
                typed.types().get(*argument),
                Some(UnitTypeKind::TypeParameter(parameter)) if *parameter == owner_parameter
            ) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(())
        }
        _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

/// 闭合实际参数仍可能把同一 nominal declaration 带回 field graph；这类 SCC 没有
/// SPEC-0219 的有限 descriptor，不能因为字段不再含 owner parameter 而静默放行。
fn validate_closed_nominal_recipe_cycles(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    span: Span,
    visiting: &mut BTreeSet<DeclarationId>,
) -> Result<(), LoweringError> {
    let kind = typed
        .types()
        .get(ty)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match kind {
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            if !visiting.insert(*declaration) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let nominal = typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if nominal.kind() != NominalKind::EnumClass && typed.runtime_field_layout(ty).is_some()
            {
                for (concrete, field) in
                    resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)?
                        .into_iter()
                        .zip(nominal.fields())
                {
                    validate_closed_nominal_recipe_cycles(typed, concrete, field.span(), visiting)?;
                }
                visiting.remove(declaration);
                return Ok(());
            }
            let substitutions = nominal
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments.iter().copied())
                .collect::<BTreeMap<_, _>>();
            for (template, field_span) in nominal
                .fields()
                .iter()
                .map(|field| (field.ty(), field.span()))
                .chain(nominal.enum_cases().iter().flat_map(|case| {
                    case.payloads()
                        .iter()
                        .map(|payload| (payload.ty(), payload.span()))
                }))
            {
                let concrete =
                    resolve_closed_recipe_type(typed, template, &substitutions, field_span)?;
                validate_closed_nominal_recipe_cycles(typed, concrete, field_span, visiting)?;
            }
            visiting.remove(declaration);
            Ok(())
        }
        UnitTypeKind::Intrinsic { arguments, .. } => {
            for &argument in arguments {
                validate_closed_nominal_recipe_cycles(typed, argument, span, visiting)?;
            }
            Ok(())
        }
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => {
            validate_closed_nominal_recipe_cycles(typed, *inner, span, visiting)
        }
        UnitTypeKind::EnumCase { root, .. } => {
            validate_closed_nominal_recipe_cycles(typed, *root, span, visiting)
        }
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Function { .. }
        | UnitTypeKind::TypeParameter(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => Ok(()),
    }
}

/// closed recipe 的 SCC 检查需要完整替换容器内参数，但不因此扩张通用 callable
/// specialization 支持面。
fn resolve_closed_recipe_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    let resolve_arguments = |arguments: &[UnitTypeId]| {
        arguments
            .iter()
            .map(|argument| resolve_closed_recipe_type(typed, *argument, substitutions, span))
            .collect::<Result<Vec<_>, _>>()
    };
    let concrete =
        match typed.types().get(ty) {
            Some(UnitTypeKind::TypeParameter(parameter)) => {
                return substitutions
                    .get(parameter)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span));
            }
            Some(UnitTypeKind::Nullable(inner)) => UnitTypeKind::Nullable(
                resolve_closed_recipe_type(typed, *inner, substitutions, span)?,
            ),
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => UnitTypeKind::Nominal {
                declaration: *declaration,
                arguments: resolve_arguments(arguments)?,
            },
            Some(UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            }) => UnitTypeKind::Intrinsic {
                constructor: *constructor,
                arguments: resolve_arguments(arguments)?,
            },
            Some(UnitTypeKind::EnumCase { case, root }) => UnitTypeKind::EnumCase {
                case: *case,
                root: resolve_closed_recipe_type(typed, *root, substitutions, span)?,
            },
            Some(UnitTypeKind::Function { .. }) => return Ok(ty),
            Some(kind) if contains_type_parameter(typed, kind) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Some(_) => return Ok(ty),
            None => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
    Ok(typed
        .types()
        .find(&concrete)
        // SCC 检查只依赖 declaration graph；frontend 未 intern 中间 closed
        // constructor 时保留模板 identity，不能把有限 DAG 误报成 missing fact。
        .unwrap_or(ty))
}
