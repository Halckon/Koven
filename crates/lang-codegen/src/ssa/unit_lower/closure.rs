//! compilation-unit lambda 的 function pointer/concrete closure、thunk 与调用 lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        Namespace, SourceUnitId, SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode, UnitClosureCaptureSource},
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, Copyability, ParameterMode, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    FunctionPlan, LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value,
    resolve_concrete_type, span_contains, span_key, strictly_contains,
    type_lower::UnitTypeLowering,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        ClosureCaptureMode as SsaCaptureMode, ClosureCaptureType, EntityType, FunctionId, LoanKind,
        Module, Operation, Origin, SsaTypeId,
    },
};


pub(super) type CallableLayouts = BTreeMap<
    (
        crate::ssa::lowering_support::callable_instances::SourceToken,
        UnitExpressionId,
    ),
    CallableLayout,
>;

pub(super) type CallablePlanKey = (FunctionId, UnitExpressionId);

#[derive(Clone, Copy)]
pub(super) struct CapturePlan {
    symbol: UnitSymbolId,
    unit_ty: UnitTypeId,
    ty: SsaTypeId,
    storage: SsaTypeId,
    mode: ClosureCaptureMode,
    effect: ClosureCaptureEffect,
    span: Span,
}

pub(super) struct CallableLayout {
    identity: String,
    parameters: Vec<EntityType>,
    returns: Vec<SsaTypeId>,
    source_unit: SourceUnitId,
    pub(super) callable: SsaTypeId,
    environment: Option<SsaTypeId>,
    body: StatementId,
    span: Span,
    parameter_symbols: Vec<UnitSymbolId>,
    parameter_modes: Vec<ParameterMode>,
    return_type: UnitTypeId,
    result_expression: Option<ExpressionId>,
    captures: Vec<CapturePlan>,
    substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
}

#[derive(Clone)]
pub(super) struct CallablePlan {
    pub(super) scope: FunctionId,
    pub(super) source_token: crate::ssa::lowering_support::callable_instances::SourceToken,
    pub(super) source_unit: SourceUnitId,
    pub(super) expression: UnitExpressionId,
    pub(super) callable: SsaTypeId,
    pub(super) environment: Option<SsaTypeId>,
    pub(super) thunk: FunctionId,
    pub(super) body: StatementId,
    pub(super) span: Span,
    pub(super) parameter_symbols: Vec<UnitSymbolId>,
    pub(super) parameter_modes: Vec<ParameterMode>,
    pub(super) return_type: UnitTypeId,
    pub(super) result_expression: Option<ExpressionId>,
    pub(super) captures: Vec<CapturePlan>,
    pub(super) substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
    pub(super) static_self: Option<UnitTypeId>,
}

/// The lambdas lowered inside `scope`, as the refinement layer sees them (ADR-0034).
pub(super) fn refine_callables(
    plans: &BTreeMap<CallablePlanKey, CallablePlan>,
    scope: FunctionId,
) -> BTreeMap<UnitExpressionId, crate::ssa::refine::RefineCallable> {
    plans
        .values()
        .filter(|plan| plan.scope == scope)
        .map(|plan| {
            (
                plan.expression,
                crate::ssa::refine::RefineCallable {
                    callable: plan.callable,
                    thunk: plan.thunk,
                    captures: plan
                        .captures
                        .iter()
                        .map(|capture| crate::ssa::refine::RefineCapture {
                            symbol: capture.symbol,
                            ty: capture.ty,
                            mode: capture.mode,
                            effect: capture.effect,
                        })
                        .collect(),
                },
            )
        })
        .collect()
}

/// A lambda thunk body description for the refinement layer (SPEC-0370, ADR-0034).
pub(super) fn refine_thunk(plan: &CallablePlan) -> crate::ssa::refine::RefineThunk {
    crate::ssa::refine::RefineThunk {
        expression: plan.expression,
        body: plan.body,
        result: plan.result_expression,
        environment: plan.environment,
        captures: plan
            .captures
            .iter()
            .map(|capture| crate::ssa::refine::RefineThunkCapture {
                symbol: capture.symbol,
                ty: capture.unit_ty,
                storage: capture.storage,
                mode: capture.mode,
                span: capture.span,
            })
            .collect(),
    }
}

pub(super) fn declare(
    module: &mut Module,
    plans: &[FunctionPlan],
    layouts: CallableLayouts,
) -> Result<BTreeMap<CallablePlanKey, CallablePlan>, LoweringError> {
    let owners = plans
        .iter()
        .map(|plan| (plan.instance.source_token(), plan.id))
        .collect::<BTreeMap<_, _>>();
    let mut closures = BTreeMap::new();
    for ((source, expression), layout) in layouts {
        let scope = *owners
            .get(&source)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, layout.span))?;
        let never_returns = module
            .callable_signature(layout.callable)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, layout.span))?
            .never_returns;
        let thunk = module
            .add_function(
                format!("{}.thunk", layout.identity),
                layout.returns,
                Origin::Source(layout.span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, layout.span))?;
        let function = module
            .function_mut(thunk)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, layout.span))?;
        function.never_returns = never_returns;
        function
            .add_block(layout.parameters, Origin::Source(layout.span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, layout.span))?;
        closures.insert(
            (scope, expression),
            CallablePlan {
                scope,
                source_token: source,
                source_unit: layout.source_unit,
                expression,
                callable: layout.callable,
                environment: layout.environment,
                thunk,
                body: layout.body,
                span: layout.span,
                parameter_symbols: layout.parameter_symbols,
                parameter_modes: layout.parameter_modes,
                return_type: layout.return_type,
                result_expression: layout.result_expression,
                captures: layout.captures,
                substitutions: layout.substitutions,
                static_self: layout.static_self,
            },
        );
    }
    Ok(closures)
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_callable_literal(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let plan = self
            .callable_plans
            .get(&(
                self.function.id,
                UnitExpressionId::new(self.source_unit, expression),
            ))
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        // Captured closures are lowered by `ssa::refine` (SPEC-0370 / SPEC-0393).
        if let Some(capture) = plan.captures.first() {
            let err_span = if self.closure_binding_context {
                capture.span
            } else {
                span
            };
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, err_span));
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::FunctionAddress { target: plan.thunk },
                vec![EntityType::Value(plan.callable)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    pub(super) fn closure_origin(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitExpressionId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Expression::Lambda { .. } => {
                Ok(Some(UnitExpressionId::new(self.source_unit, expression)))
            }
            Expression::Group { expression } => self.closure_origin(*expression),
            _ => Ok(None),
        }
    }
}

pub(super) mod layout {
    //! Callable layouts precede source signatures; thunk declarations follow them.

    use super::*;
    use lang_frontend::parser::Statement;

    #[cfg(test)]
    mod tests {
        //! Synthetic invalid SSA identities only run in official CI; old witnesses remain.
        use super::shared_capture_storage;
        use crate::ssa::{
            LoweringError, LoweringErrorKind,
            model::{Program, SsaTypeId, SsaTypeKind},
        };
        use lang_frontend::source::SourceMap;

        #[test]
        fn checked_shared_capture_storage_preserves_local_type_and_interning() {
            let mut sources = SourceMap::new();
            let source = sources.add_source("capture.ko", "{ owner }").unwrap();
            let span = sources.span(source, 0, 9).unwrap();
            let mut program = Program::default();
            let module_id = program.add_module("capture");
            let module = program.module_mut(module_id).unwrap();
            let target = module.intern_type(SsaTypeKind::Boolean);
            let storage = shared_capture_storage(module, target, span).unwrap();
            assert_eq!(
                module.type_kind(storage),
                Some(&SsaTypeKind::SharedReference { target })
            );
            assert_eq!(storage.module(), module_id);
            assert_eq!(
                shared_capture_storage(module, target, span).unwrap(),
                storage
            );
            assert_eq!(module.types.len(), 2);
        }

        #[test]
        fn invalid_shared_capture_identity_maps_to_exact_lowering_error_without_mutation() {
            let mut sources = SourceMap::new();
            let source = sources
                .add_source("capture.ko", "prefix { owner }")
                .unwrap();
            let span = sources.span(source, 7, 16).unwrap();
            let mut program = Program::default();
            let module_id = program.add_module("capture");
            let foreign_module_id = program.add_module("foreign");
            let foreign_module = program
                .module_mut(foreign_module_id)
                .unwrap()
                .intern_type(SsaTypeKind::Boolean);
            let mut foreign_program = Program::default();
            let foreign_owner_id = foreign_program.add_module("capture");
            let foreign_owner = foreign_program
                .module_mut(foreign_owner_id)
                .unwrap()
                .intern_type(SsaTypeKind::Boolean);
            assert_eq!(module_id.index(), foreign_owner_id.index());
            let module = program.module_mut(module_id).unwrap();
            module.intern_type(SsaTypeKind::Boolean);
            let unknown = SsaTypeId {
                module: module_id,
                index: module.types.len(),
            };
            for invalid in [foreign_module, foreign_owner, unknown] {
                let before = module.types.clone();
                assert_eq!(
                    shared_capture_storage(module, invalid, span),
                    Err(LoweringError {
                        kind: LoweringErrorKind::InvalidModel,
                        span: Some(span),
                    })
                );
                assert_eq!(module.types, before);
            }
        }
    }

    fn shared_capture_storage(
        module: &mut Module,
        target: SsaTypeId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        module
            .add_shared_reference_type(target)
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
    }

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
                            fact.origin()
                                == lang_frontend::ownership_checking::UnitCallableOrigin::Lambda(id)
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
                let mut parameter_symbols = Vec::with_capacity(callable_parameters.len());
                let mut parameter_modes = Vec::with_capacity(callable_parameters.len());
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
                    let supported_storage_borrow = parameter.mode() == ParameterMode::Borrow
                        && super::super::type_lower::is_supported_storage_type(typed, concrete);
                    if parameter.mode() == ParameterMode::Inout
                        || (!supported_value
                            && !supported_storage_borrow
                            && typed.copyability(concrete) != Copyability::Copyable)
                        || (parameter.mode() == ParameterMode::Borrow
                            && builtin_type(typed, concrete) == Some(BuiltinType::Unit))
                    {
                        return Err(lowering_error(
                            LoweringErrorKind::UnsupportedNode,
                            *parameter_span,
                        ));
                    }
                    let symbol = names
                        .names()
                        .source_units()
                        .get(instance.source_unit().index())
                        .and_then(|source| {
                            source.resolution().symbols().iter().find(|symbol| {
                                symbol.span() == *parameter_span
                                    && symbol.kind() == SymbolKind::LambdaParameter
                            })
                        })
                        .and_then(|parameter_symbol| {
                            typed.body_symbol_types().keys().copied().find(|symbol| {
                                symbol.source_unit() == instance.source_unit()
                                    && symbol.symbol() == parameter_symbol.id()
                            })
                        })
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, *parameter_span)
                        })?;
                    if supported_value && typed.copyability(concrete) == Copyability::MoveOnly {
                        move_only_value_parameters.push(symbol);
                    }
                    parameter_symbols.push(symbol);
                    parameter_modes.push(parameter.mode());
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
                let result_builtin = builtin_type(typed, return_type);
                let never_returns = result_builtin == Some(BuiltinType::Nothing);
                let returns_value = !matches!(
                    result_builtin,
                    Some(BuiltinType::Unit | BuiltinType::Nothing)
                );
                let result_expression = returns_value
                    .then(|| lambda_tail_expression(parsed, *body, span))
                    .transpose()?;
                if returns_value && typed.copyability(return_type) == Copyability::MoveOnly {
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
                let callable_returns = if returns_value {
                    vec![types.intern(module, typed, return_type, span)?]
                } else {
                    Vec::new()
                };
                let mut captures = Vec::new();
                let mut environment_fields = Vec::new();
                let mut capture_types = Vec::new();
                let capture_facts = owned.captures_of(id).collect::<Vec<_>>();

                for capture in capture_facts {
                    if capture.borrow_kind()
                        == Some(lang_frontend::ownership_checking::LoanKind::Exclusive)
                    {
                        return Err(lowering_error(
                            LoweringErrorKind::UnsupportedNode,
                            capture.reference_span(),
                        ));
                    }
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
                            shared_capture_storage(module, ty, span)?,
                        ),
                    };
                    environment_fields.push(storage);
                    capture_types.push(ClosureCaptureType { mode, ty });
                    captures.push(CapturePlan {
                        symbol,
                        unit_ty: concrete,
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
                let signature = crate::ssa::model::CallableSignature {
                    parameters: callable_parameter_types.clone(),
                    returns: callable_returns.clone(),
                    never_returns,
                };
                let (callable, environment, thunk_parameters) = if captures.is_empty() {
                    let callable = module
                        .add_function_pointer_signature(signature)
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    (callable, None, callable_parameter_types)
                } else {
                    let environment = module
                        .add_aggregate_type(format!("{identity}.environment"), environment_fields)
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    let callable = module
                        .add_concrete_closure_signature(
                            format!("{identity}.closure"),
                            signature,
                            environment,
                            capture_types,
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    let mut thunk_parameters = vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: environment,
                    }];
                    thunk_parameters.extend(callable_parameter_types);
                    (callable, Some(environment), thunk_parameters)
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
                            environment,
                            body: *body,
                            span,
                            parameter_symbols,
                            parameter_modes,
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

    fn lambda_tail_expression(
        parsed: &lang_frontend::parser::ParsedFile,
        body: StatementId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        let body = parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::LambdaBody { elements } = body.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let last = elements
            .last()
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let last = parsed
            .ast()
            .statements()
            .get(last)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::Expression { expression } = last.payload() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        Ok(*expression)
    }

    fn direct_reference_symbol(
        parsed: &lang_frontend::parser::ParsedFile,
        expression: ExpressionId,
        references: &BTreeMap<(usize, usize), UnitSymbolId>,
    ) -> Option<UnitSymbolId> {
        let node = parsed.ast().expressions().get(expression).ok()?;
        match node.payload() {
            Expression::Name => references.get(&span_key(node.span())).copied(),
            Expression::Group { expression } => {
                direct_reference_symbol(parsed, *expression, references)
            }
            _ => None,
        }
    }
}
