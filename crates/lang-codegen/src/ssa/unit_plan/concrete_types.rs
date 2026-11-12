//! 仅查找 frontend 已发布的 canonical type，保留 preflight 与正式替换的各自边界。

use std::collections::BTreeMap;

use lang_frontend::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{
        CompilationUnitTypes, IntrinsicTypeConstructor, NominalKind, UnitFunctionParameterType,
        UnitTypeId, UnitTypeKind,
    },
};

use super::lowering_error;
use crate::ssa::{LoweringError, LoweringErrorKind};

/// 只查找 frontend 已发布的 canonical specialization，不创建新的 concrete type。
pub(super) fn specialize_preflight_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    static_self: Option<UnitTypeId>,
    substitutions: Option<&BTreeMap<UnitSymbolId, UnitTypeId>>,
) -> Option<UnitTypeId> {
    match typed.types().get(ty)? {
        UnitTypeKind::TypeParameter(parameter) => substitutions
            .and_then(|substitutions| substitutions.get(parameter).copied())
            .or(Some(ty)),
        UnitTypeKind::StaticSelf(_) => static_self,
        UnitTypeKind::Nullable(inner) => {
            let inner = specialize_preflight_type(typed, *inner, static_self, substitutions)?;
            typed.types().find(&UnitTypeKind::Nullable(inner))
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|&argument| {
                    specialize_preflight_type(typed, argument, static_self, substitutions)
                })
                .collect::<Option<Vec<_>>>()?;
            typed.types().find(&UnitTypeKind::Nominal {
                declaration: *declaration,
                arguments,
            })
        }
        UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|&argument| {
                    specialize_preflight_type(typed, argument, static_self, substitutions)
                })
                .collect::<Option<Vec<_>>>()?;
            typed.types().find(&UnitTypeKind::Intrinsic {
                constructor: *constructor,
                arguments,
            })
        }
        _ => Some(ty),
    }
}

pub(crate) fn resolve_concrete_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::StaticSelf(_)) => {
            static_self.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Function {
            move_only,
            parameters,
            return_type,
        }) => {
            let parameters = parameters
                .iter()
                .map(|parameter| {
                    let ty = resolve_concrete_type(
                        typed,
                        parameter.ty(),
                        substitutions,
                        static_self,
                        span,
                    )?;
                    Ok(UnitFunctionParameterType::new(parameter.mode(), ty))
                })
                .collect::<Result<Vec<_>, LoweringError>>()?;
            let return_type =
                resolve_concrete_type(typed, *return_type, substitutions, static_self, span)?;
            typed
                .types()
                .find(&UnitTypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nullable(inner)) => {
            let inner = resolve_concrete_type(typed, *inner, substitutions, static_self, span)?;
            typed
                .types()
                .find(&UnitTypeKind::Nullable(inner))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) => {
            let arguments = arguments
                .iter()
                .map(|argument| {
                    resolve_direct_type_argument(typed, *argument, substitutions, static_self, span)
                })
                .collect::<Result<Vec<_>, _>>()?;
            typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: *declaration,
                    arguments,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(
            kind @ UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            },
        ) if matches!(
            constructor,
            IntrinsicTypeConstructor::View
                | IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList
        ) && contains_type_parameter(typed, kind) =>
        {
            // Only replace a direct element argument; frontend owns canonical type publication.
            let [argument] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let argument =
                resolve_direct_type_argument(typed, *argument, substitutions, static_self, span)?;
            typed
                .types()
                .find(&UnitTypeKind::Intrinsic {
                    constructor: *constructor,
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

pub(super) fn resolve_direct_type_argument(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::StaticSelf(_)) => {
            static_self.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

/// 只把 exact enum root 的 direct/closed payload 映射为 frontend 已发布的类型。
/// 所有 case 都参与布局；模板 refinement 不成为任一 concrete root 的别名。
pub(crate) fn resolve_enum_runtime_payload_types(
    typed: &CompilationUnitTypes,
    root: UnitTypeId,
    span: Span,
) -> Result<Vec<Vec<UnitTypeId>>, LoweringError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(root)
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    let nominal = typed
        .signatures()
        .declaration(*declaration)
        .and_then(|signature| signature.nominal())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if nominal.kind() != NominalKind::EnumClass
        || nominal.declaration() != *declaration
        || nominal.type_parameters().len() != arguments.len()
    {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    for &argument in arguments {
        let kind = typed
            .types()
            .get(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        // A valid generic callable template is an unmigrated boundary, not a missing fact.
        if contains_type_parameter(typed, kind) || matches!(kind, UnitTypeKind::StaticSelf(_)) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    let substitutions = nominal
        .type_parameters()
        .iter()
        .copied()
        .zip(arguments.iter().copied())
        .collect::<BTreeMap<_, _>>();
    nominal
        .enum_cases()
        .iter()
        .map(|case| {
            if !matches!(typed.types().get(case.case_type()),
            Some(UnitTypeKind::EnumCase { case: symbol, root })
                if *symbol == case.type_symbol() && *root == nominal.ty())
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    case.name_span(),
                ));
            }
            case.payloads()
                .iter()
                .map(|payload| {
                    resolve_direct_type_argument(
                        typed,
                        payload.ty(),
                        &substitutions,
                        None,
                        payload.span(),
                    )
                })
                .collect()
        })
        .collect()
}

pub(super) fn contains_type_parameter(typed: &CompilationUnitTypes, kind: &UnitTypeKind) -> bool {
    let contains = |ty| {
        typed
            .types()
            .get(ty)
            .is_some_and(|kind| contains_type_parameter(typed, kind))
    };
    match kind {
        UnitTypeKind::TypeParameter(_) => true,
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => contains(*inner),
        UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        } => parameters.iter().any(|parameter| contains(parameter.ty())) || contains(*return_type),
        UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. } => {
            arguments.iter().copied().any(contains)
        }
        UnitTypeKind::EnumCase { root, .. } => contains(*root),
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => false,
    }
}

pub(in crate::ssa::unit_plan) mod deinit {
    //! Hidden resource bodies are reached by runtime storage, never by source calls.

    use super::super::*;
    use lang_frontend::{parser::Expression, type_checking::ParameterMode};

    pub(in crate::ssa::unit_plan) fn template(
        nominal: &UnitNominalSignature,
        parsed: &ParsedFile,
    ) -> Result<Option<UnitFunctionTemplate>, LoweringError> {
        let Some(descriptor) = nominal.deinit() else {
            return Ok(None);
        };
        let node = parsed
            .ast()
            .items()
            .get(descriptor.item().item())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        if descriptor.owner() != nominal.declaration()
            || descriptor.item().source_unit() != nominal.symbol().source_unit()
            || descriptor.body().source_unit() != descriptor.item().source_unit()
            || descriptor.receiver_type() != nominal.ty()
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || !matches!(node.payload(), Item::Deinit { body, .. } if *body == descriptor.body().statement())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
        }
        Ok(Some(UnitFunctionTemplate {
            target: UnitCallableTarget::Declaration(descriptor.owner()),
            source_unit: descriptor.item().source_unit(),
            item: descriptor.item().item(),
            type_parameters: Vec::new(),
            owner: Some(descriptor.owner()),
            span: node.span(),
            deinit: true,
        }))
    }

    /// Direct invocation selects a resource body only through the proved callable origin.
    /// A completed receiver prefix alone cannot admit a lambda whose arguments never dispatch.
    pub(in crate::ssa::unit_plan) fn directly_invoked_lambda(
        owned: &CompilationUnitOwnership,
        parsed: &ParsedFile,
        call: &lang_frontend::type_checking::UnitCallDescriptor,
    ) -> Result<Option<UnitExpressionId>, LoweringError> {
        if owned.call_may_dispatch(call.expression()) != Some(true) {
            return Ok(None);
        }
        let node = parsed
            .ast()
            .expressions()
            .get(call.expression().expression())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let Expression::Call { callee, .. } = node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
        };
        let callee = UnitExpressionId::new(call.expression().source_unit(), *callee);
        let Some(fact) = owned.callable_origin(callee) else {
            return Ok(None);
        };
        let lang_frontend::ownership_checking::UnitCallableOrigin::Lambda(lambda) = fact.origin()
        else {
            return Ok(None);
        };
        if lambda.source_unit() != call.expression().source_unit()
            || owned.closure(lambda).is_none()
            || !owned.lambda_was_entered(lambda)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
        }
        Ok(Some(lambda))
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa::unit_plan) fn plan_instance_deinits(
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        names: &ValidatedCompilationUnitNames,
        parsed: &ParsedFile,
        template: &UnitFunctionTemplate,
        key: &UnitFunctionInstanceKey,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        resource_lambdas: &BTreeSet<UnitExpressionId>,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
    ) -> Result<(), LoweringError> {
        let mut roots = Vec::new();
        if let Some(owner) = key.deinit_owner() {
            let descriptor = typed
                .signatures()
                .declaration(owner)
                .and_then(|signature| signature.nominal())
                .and_then(|nominal| nominal.deinit())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, template.span))?;
            roots.push((descriptor.receiver_type(), template.span));
        } else {
            let callable = unit_callable_signature(typed, key.target())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, template.span))?;
            roots.extend(
                callable
                    .parameters()
                    .iter()
                    .map(|parameter| (parameter.ty(), parameter.span())),
            );
            roots.push((callable.return_type(), template.span));
            if let Some(receiver) = callable.receiver() {
                roots.push((receiver.ty(), receiver.declaration_span()));
            }
        }
        for (&expression, &ty) in typed.expression_types() {
            if expression.source_unit() != template.source_unit {
                continue;
            }
            let span = parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, template.span))?
                .span();
            if span_contains(template.span, span) {
                roots.push((ty, span));
            }
        }
        // A nullable/otherwise annotated local can have no expression with its binding type.
        // Consume its published symbol type too, so an unused unsupported resource cannot vanish.
        for (&symbol, &ty) in typed.body_symbol_types() {
            if symbol.source_unit() != template.source_unit {
                continue;
            }
            let span = names.names().source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()
                .get(symbol.symbol().index())
                .map(|symbol| symbol.span());
            if let Some(span) = span.filter(|&span| span_contains(template.span, span)) {
                roots.push((ty, span));
            }
        }
        let lambda_spans = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(expression, node)| {
                (matches!(node.payload(), Expression::Lambda { .. })
                    && span_contains(template.span, node.span()))
                .then_some((
                    UnitExpressionId::new(template.source_unit, expression),
                    node.span(),
                ))
            })
            .collect::<Vec<_>>();
        if template.deinit && !lambda_spans.is_empty() {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                lambda_spans[0].1,
            ));
        }
        let unentered_lambda_spans = lambda_spans
            .iter()
            .filter_map(|&(lambda, span)| (!owned.lambda_was_entered(lambda)).then_some(span))
            .collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        for (ty, span) in roots {
            if unentered_lambda_spans
                .iter()
                .any(|&lambda_span| span_contains(lambda_span, span))
            {
                continue;
            }
            // This pass adds destructor reachability, not general runtime type validation.
            // In particular, a generic source callable has a pure function type whose
            // unresolved parameters must remain under the existing callable planner.
            if typed.is_resource_type(ty) == Some(false) {
                continue;
            }
            let concrete =
                resolve_concrete_type(typed, ty, substitutions, key.static_self(), span)?;
            if !contains_resource(typed, concrete, &mut BTreeSet::new(), span)? {
                continue;
            }
            if lambda_spans.iter().any(|&(lambda, lambda_span)| {
                span_contains(lambda_span, span) && !resource_lambdas.contains(&lambda)
            }) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            schedule_layout(typed, concrete, pending, &mut visited, span)?;
        }
        Ok(())
    }

    fn schedule_layout(
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        visited: &mut BTreeSet<UnitTypeId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !contains_resource(typed, ty, &mut BTreeSet::new(), span)? || !visited.insert(ty) {
            return Ok(());
        }
        // Existing nullable class handles own the same inner resource conditionally.
        // Keep unsupported Rc/Box resource recipes under their old rejection.
        if let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(ty)
            && let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().get(*inner)
            && typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nominal| nominal.kind() == NominalKind::Class)
        {
            return schedule_layout(typed, *inner, pending, visited, span);
        }
        // Map-only nullable storage conditionally owns the same inner map.
        // Other inline nullable layouts keep their existing rejection.
        if let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(ty)
            && let Some(UnitTypeKind::Intrinsic { constructor, .. }) = typed.types().get(*inner)
            && matches!(
                constructor,
                lang_frontend::type_checking::IntrinsicTypeConstructor::Map
                    | lang_frontend::type_checking::IntrinsicTypeConstructor::MutableMap
            )
        {
            return schedule_layout(typed, *inner, pending, visited, span);
        }
        // Sequential provider storage owns each element; its existing drop glue must
        // reach the same hidden bodies as a direct owner or a concrete value field.
        if let Some(UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        }) = typed.types().get(ty)
            && matches!(
                constructor,
                lang_frontend::type_checking::IntrinsicTypeConstructor::Map
                    | lang_frontend::type_checking::IntrinsicTypeConstructor::MutableMap
            )
        {
            let [key, value] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            schedule_layout(typed, *key, pending, visited, span)?;
            return schedule_layout(typed, *value, pending, visited, span);
        }
        if let Some(UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        }) = typed.types().get(ty)
            && matches!(
                constructor,
                lang_frontend::type_checking::IntrinsicTypeConstructor::Array
                    | lang_frontend::type_checking::IntrinsicTypeConstructor::List
                    | lang_frontend::type_checking::IntrinsicTypeConstructor::MutableList
            )
        {
            let [element] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            return schedule_layout(typed, *element, pending, visited, span);
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = typed.types().get(ty)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(nominal.kind(), NominalKind::Class | NominalKind::ValueClass)
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
            || !nominal.interfaces().is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some(descriptor) = nominal.deinit() {
            if descriptor.owner() != *declaration || descriptor.receiver_type() != ty {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            pending.insert(UnitFunctionInstanceKey::for_deinit(*declaration));
        }
        for field in nominal.fields() {
            schedule_layout(typed, field.ty(), pending, visited, field.span())?;
        }
        Ok(())
    }

    /// Unknown generic classification is not evidence of being pure-memory. Inspect only
    /// the concrete published field recipe to reject unsupported resource wrappers.
    fn contains_resource(
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
        visiting: &mut BTreeSet<UnitTypeId>,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if let Some(resource) = typed.is_resource_type(ty) {
            return Ok(resource);
        }
        if !visiting.insert(ty) {
            return Ok(false);
        }
        let result = match typed.types().get(ty) {
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                let nominal = typed
                    .signatures()
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let fields = resolve_nominal_runtime_field_types(typed, ty, nominal, arguments)?;
                let mut resource = false;
                for field in fields {
                    resource |= contains_resource(typed, field, visiting, span)?;
                }
                resource
            }
            Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner)) => {
                contains_resource(typed, *inner, visiting, span)?
            }
            Some(UnitTypeKind::Intrinsic { arguments, .. }) => {
                let mut resource = false;
                for &argument in arguments {
                    resource |= contains_resource(typed, argument, visiting, span)?;
                }
                resource
            }
            // Function type classification says nothing about its capture environment;
            // resource expressions/bindings inside the lambda are rejected above instead.
            Some(_) => false,
            None => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        visiting.remove(&ty);
        Ok(result)
    }
}
