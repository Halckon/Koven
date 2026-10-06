//! Callable layouts precede source signatures; thunk declarations follow them.

use super::*;

pub(in crate::ssa::unit_lower) fn declare(
    module: &mut Module,
    parsed_by_source: &[&lang_frontend::parser::ParsedFile],
    instances: &[crate::ssa::unit_plan::UnitPlannedInstance],
    names: &ValidatedCompilationUnitNames,
    typed: &lang_frontend::type_checking::CompilationUnitTypes,
    owned: &lang_frontend::ownership_checking::CompilationUnitOwnership,
    types: &mut UnitTypeLowering,
) -> Result<CallableLayouts, LoweringError> {
    let mut closures = BTreeMap::new();
    for (ordinal, instance) in instances.iter().enumerate() {
        let references =
            super::super::symbol_references(names, instance.source_unit(), Namespace::Value);
        let parsed = parsed_by_source
            .get(instance.source_unit().index())
            .copied()
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let lambdas = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(expression, node)| {
                let id = UnitExpressionId::new(instance.source_unit(), expression);
                (matches!(node.payload(), Expression::Lambda { .. })
                    // Static descriptors also include lambdas after a terminating prefix.
                    && owned.callable_origin(id).is_some_and(|fact| {
                        fact.origin() == lang_frontend::ownership_checking::UnitCallableOrigin::Lambda(id)
                    }))
                .then_some((expression, node.span()))
            })
            .filter(|(_, span)| span_contains(instance.span(), *span))
            .collect::<Vec<_>>();
        for (expression, span) in &lambdas {
            if lambdas
                .iter()
                .any(|(other, owner)| other != expression && strictly_contains(*owner, *span))
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, *span));
            }
        }
        for (expression, span) in lambdas {
            let id = UnitExpressionId::new(instance.source_unit(), expression);
            let node = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Lambda {
                opener_span,
                parameters,
                arrow_span,
                body,
                ..
            } = node.payload()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let descriptor = owned
                .closure(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ty = typed
                .expression_type(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let UnitTypeKind::Function {
                parameters: callable_parameters,
                return_type,
                ..
            } = typed
                .types()
                .get(ty)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let return_type = resolve_concrete_type(
                typed,
                *return_type,
                instance.substitutions(),
                instance.key().static_self(),
                span,
            )?;
            let parameter_spans = if arrow_span.is_none()
                && parameters.is_empty()
                && callable_parameters.len() == 1
            {
                std::slice::from_ref(opener_span)
            } else {
                parameters.as_slice()
            };
            if parameter_spans.len() != callable_parameters.len() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let mut callable_parameter_types = Vec::with_capacity(callable_parameters.len());
            let mut move_only_value_parameters = Vec::new();
            for (parameter, parameter_span) in callable_parameters.iter().zip(parameter_spans) {
                let concrete = resolve_concrete_type(
                    typed,
                    parameter.ty(),
                    instance.substitutions(),
                    instance.key().static_self(),
                    *parameter_span,
                )?;
                let supported_value = parameter.mode() == ParameterMode::Value
                    && super::super::type_lower::is_supported_storage_type(typed, concrete);
                let supported_string_borrow = parameter.mode() == ParameterMode::Borrow
                    && builtin_type(typed, concrete) == Some(BuiltinType::String);
                if parameter.mode() == ParameterMode::Inout
                    || (!supported_value
                        && !supported_string_borrow
                        && typed.copyability(concrete) != Copyability::Copyable)
                    || (parameter.mode() == ParameterMode::Borrow
                        && builtin_type(typed, concrete) == Some(BuiltinType::Unit))
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        *parameter_span,
                    ));
                }
                if supported_value && typed.copyability(concrete) == Copyability::MoveOnly {
                    let symbol = owned
                        .bindings()
                        .iter()
                        .find(|binding| binding.declaration_span() == *parameter_span)
                        .map(|binding| binding.symbol())
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, *parameter_span)
                        })?;
                    move_only_value_parameters.push(symbol);
                }
                let ty = types.intern(module, typed, concrete, *parameter_span)?;
                callable_parameter_types.push(match parameter.mode() {
                    ParameterMode::Value => EntityType::Value(ty),
                    ParameterMode::Borrow => EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    },
                    ParameterMode::Inout => unreachable!("Inout was rejected above"),
                });
            }
            let result_expression = (builtin_type(typed, return_type) != Some(BuiltinType::Unit))
                .then(|| lambda_tail_expression(parsed, *body, span))
                .transpose()?;
            if typed.copyability(return_type) == Copyability::MoveOnly {
                let tail = result_expression
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let tail_node = parsed
                    .ast()
                    .expressions()
                    .get(tail)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if !matches!(tail_node.payload(), Expression::Return { .. }) {
                    let tail_id = UnitExpressionId::new(instance.source_unit(), tail);
                    let tail_type = typed
                        .expression_type(tail_id)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    let tail_type = resolve_concrete_type(
                        typed,
                        tail_type,
                        instance.substitutions(),
                        instance.key().static_self(),
                        span,
                    )?;
                    let category = typed.expression_category(tail_id);
                    let parameter_place = category
                        == Some(lang_frontend::type_checking::ExpressionCategory::Place)
                        && direct_reference_symbol(parsed, tail, &references)
                            .is_some_and(|symbol| move_only_value_parameters.contains(&symbol));
                    if (category
                        != Some(lang_frontend::type_checking::ExpressionCategory::Temporary)
                        && !parameter_place)
                        || tail_type != return_type
                    {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                }
                if !super::super::type_lower::is_supported_storage_type(typed, return_type) {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            let callable_returns = if builtin_type(typed, return_type) == Some(BuiltinType::Unit) {
                Vec::new()
            } else {
                vec![types.intern(module, typed, return_type, span)?]
            };
            let mut captures = Vec::new();
            let mut environment_fields = Vec::new();
            let mut capture_types = Vec::new();
            let capture_facts = owned.captures_of(id).collect::<Vec<_>>();

            for capture in capture_facts {
                let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.reference_span(),
                    ));
                };
                if (capture.mode() == ClosureCaptureMode::Owned
                    && (!descriptor.move_owned()
                        || !matches!(
                            capture.effect(),
                            ClosureCaptureEffect::Copy | ClosureCaptureEffect::Move
                        )))
                    || (capture.mode() == ClosureCaptureMode::Shared
                        && (descriptor.move_owned()
                            || capture.effect() != ClosureCaptureEffect::Borrow))
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.reference_span(),
                    ));
                }
                let concrete = resolve_concrete_type(
                    typed,
                    capture.ty(),
                    instance.substitutions(),
                    instance.key().static_self(),
                    capture.reference_span(),
                )?;
                if builtin_type(typed, concrete) == Some(BuiltinType::Unit) {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.reference_span(),
                    ));
                }
                let ty = types.intern(module, typed, concrete, capture.reference_span())?;
                let (mode, storage) = match capture.mode() {
                    ClosureCaptureMode::Owned => (SsaCaptureMode::Owned, ty),
                    ClosureCaptureMode::Shared => (
                        SsaCaptureMode::Shared,
                        module.intern_type(crate::ssa::model::SsaTypeKind::SharedReference {
                            target: ty,
                        }),
                    ),
                };
                environment_fields.push(storage);
                capture_types.push(ClosureCaptureType { mode, ty });
                captures.push(CapturePlan {
                    symbol,
                    ty,
                    storage,
                    mode: capture.mode(),
                    effect: capture.effect(),
                    span: capture.reference_span(),
                });
            }
            let identity = format!(
                "unit.lambda.f{}.s{}.e{}",
                ordinal,
                instance.source_unit().index(),
                expression.index()
            );
            let (callable, thunk_parameters) = if captures.is_empty() {
                let callable = module
                    .add_function_pointer_type_with_parameters(
                        callable_parameter_types.clone(),
                        callable_returns.clone(),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                (callable, callable_parameter_types)
            } else {
                let environment = module
                    .add_aggregate_type(format!("{identity}.environment"), environment_fields)
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let callable = module
                    .add_concrete_closure_type_with_parameters(
                        format!("{identity}.closure"),
                        callable_parameter_types.clone(),
                        callable_returns.clone(),
                        environment,
                        capture_types,
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let mut thunk_parameters = vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: environment,
                }];
                thunk_parameters.extend(callable_parameter_types);
                (callable, thunk_parameters)
            };
            if closures
                .insert(
                    (instance.source_token(), id),
                    CallableLayout {
                        identity,
                        returns: callable_returns,
                        parameters: thunk_parameters,
                        source_unit: instance.source_unit(),
                        callable,
                        body: *body,
                        span,
                        parameter_spans: parameter_spans.to_vec(),
                        return_type,
                        result_expression,
                        captures,
                        substitutions: instance.substitutions().clone(),
                        static_self: instance.key().static_self(),
                    },
                )
                .is_some()
            {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
        }
    }
    Ok(closures)
}
