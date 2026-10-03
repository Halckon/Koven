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

/// SPEC-0191 当前只递归接受由 `List`、单参数 ordinary class 与 direct owner parameter 组成的
/// 有限 recipe；nullable/function/其他 intrinsic 与非 class nominal 继续保持门禁。
fn supported_nested_runtime_field_recipe(
    typed: &CompilationUnitTypes,
    owner: &UnitNominalSignature,
    template: UnitTypeId,
) -> bool {
    if let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(template) {
        return direct_owner_type_parameter(typed, owner, *inner);
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
