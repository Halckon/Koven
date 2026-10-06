//! Hidden resource bodies are reached by runtime storage, never by source calls.

use super::*;
use lang_frontend::{parser::Expression, type_checking::ParameterMode};

pub(super) fn template(
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

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_instance_deinits(
    typed: &CompilationUnitTypes,
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
    let mut visited = BTreeSet::new();
    for (ty, span) in roots {
        // This pass adds destructor reachability, not general runtime type validation.
        // In particular, a generic source callable has a pure function type whose
        // unresolved parameters must remain under the existing callable planner.
        if typed.is_resource_type(ty) == Some(false) {
            continue;
        }
        let concrete = resolve_concrete_type(typed, ty, substitutions, key.static_self(), span)?;
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
    // Keep inline nullable and unsupported Rc/Box resource recipes under their old rejection.
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
    // Sequential provider storage owns each element; its existing drop glue must
    // reach the same hidden bodies as a direct owner or a concrete value field.
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
