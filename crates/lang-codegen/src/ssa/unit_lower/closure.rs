//! compilation-unit lambda 的 function pointer/concrete closure、thunk 与调用 lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        Namespace, SourceUnitId, SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, UnitClosureCaptureSource, UnitDropFact,
        UnitDropTarget,
    },
    parser::{Expression, Statement},
    source::Span,
    type_checking::{
        BuiltinType, Copyability, ParameterMode, UnitCallDescriptor, UnitExpressionId, UnitTypeId,
        UnitTypeKind,
    },
};

use super::{
    FunctionPlan, LoweredValue, UnitExpressionLowerer, builtin_type, call::LoweredCallArguments,
    lowering_error, require_value, resolve_concrete_type, span_key, type_lower::UnitTypeLowering,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        ClosureCaptureMode as SsaCaptureMode, ClosureCaptureOperand, ClosureCaptureType, EntityId,
        EntityType, FunctionId, LoanKind, Module, Operation, Origin, SsaTypeId, TerminatorKind,
    },
};

pub(super) type CallablePlanKey = (FunctionId, UnitExpressionId);

#[derive(Clone, Copy)]
pub(super) struct CapturePlan {
    symbol: UnitSymbolId,
    ty: SsaTypeId,
    effect: ClosureCaptureEffect,
    span: Span,
}

#[derive(Clone)]
pub(super) struct CallablePlan {
    pub(super) scope: FunctionId,
    pub(super) source_unit: SourceUnitId,
    pub(super) expression: UnitExpressionId,
    pub(super) callable: SsaTypeId,
    pub(super) thunk: FunctionId,
    pub(super) body: StatementId,
    pub(super) span: Span,
    pub(super) parameter_spans: Vec<Span>,
    pub(super) return_type: UnitTypeId,
    pub(super) result_expression: Option<ExpressionId>,
    pub(super) captures: Vec<CapturePlan>,
    pub(super) substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
}

pub(super) fn declare(
    module: &mut Module,
    parsed_by_source: &[&lang_frontend::parser::ParsedFile],
    plans: &[FunctionPlan],
    names: &ValidatedCompilationUnitNames,
    typed: &lang_frontend::type_checking::ValidatedCompilationUnitTypes,
    owned: &lang_frontend::ownership_checking::ValidatedCompilationUnitOwnership,
    types: &mut UnitTypeLowering,
) -> Result<BTreeMap<CallablePlanKey, CallablePlan>, LoweringError> {
    let mut closures = BTreeMap::new();
    for function in plans {
        let references =
            super::symbol_references(names, function.instance.source_unit(), Namespace::Value);
        let parsed = parsed_by_source
            .get(function.instance.source_unit().index())
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
                matches!(node.payload(), Expression::Lambda { .. })
                    .then_some((expression, node.span()))
            })
            .filter(|(_, span)| span_contains(function.instance.span(), *span))
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
            let id = UnitExpressionId::new(function.instance.source_unit(), expression);
            let node = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Lambda {
                parameters, body, ..
            } = node.payload()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let descriptor = owned
                .ownership()
                .closure(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ty = typed
                .types()
                .expression_type(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let UnitTypeKind::Function {
                move_only,
                parameters: callable_parameters,
                return_type,
            } = typed
                .types()
                .types()
                .get(ty)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let return_type = resolve_concrete_type(
                typed,
                *return_type,
                function.instance.substitutions(),
                span,
            )?;
            if parameters.len() != callable_parameters.len() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let mut callable_parameter_types = Vec::with_capacity(callable_parameters.len());
            let mut move_only_value_parameters = Vec::new();
            for (parameter, parameter_span) in callable_parameters.iter().zip(parameters) {
                let concrete = resolve_concrete_type(
                    typed,
                    parameter.ty(),
                    function.instance.substitutions(),
                    *parameter_span,
                )?;
                let supported_value = parameter.mode() == ParameterMode::Value
                    && super::type_lower::is_supported_storage_type(typed, concrete);
                if parameter.mode() == ParameterMode::Inout
                    || (!supported_value
                        && typed.types().copyability(concrete) != Copyability::Copyable)
                    || (parameter.mode() == ParameterMode::Borrow
                        && builtin_type(typed, concrete) == Some(BuiltinType::Unit))
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        *parameter_span,
                    ));
                }
                if supported_value && typed.types().copyability(concrete) == Copyability::MoveOnly {
                    let symbol = owned
                        .ownership()
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
            if typed.types().copyability(return_type) == Copyability::MoveOnly {
                let tail = result_expression
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let tail_node = parsed
                    .ast()
                    .expressions()
                    .get(tail)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if !matches!(tail_node.payload(), Expression::Return { .. }) {
                    let tail_id = UnitExpressionId::new(function.instance.source_unit(), tail);
                    let tail_type = typed
                        .types()
                        .expression_type(tail_id)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    let tail_type = resolve_concrete_type(
                        typed,
                        tail_type,
                        function.instance.substitutions(),
                        span,
                    )?;
                    let category = typed.types().expression_category(tail_id);
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
                if !super::type_lower::is_supported_storage_type(typed, return_type) {
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
            let capture_facts = owned.ownership().captures_of(id).collect::<Vec<_>>();
            if !capture_facts.is_empty() && (!descriptor.move_owned() || !*move_only) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            for capture in capture_facts {
                let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.reference_span(),
                    ));
                };
                if capture.mode() != ClosureCaptureMode::Owned
                    || !matches!(
                        capture.effect(),
                        ClosureCaptureEffect::Copy | ClosureCaptureEffect::Move
                    )
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.reference_span(),
                    ));
                }
                let concrete = resolve_concrete_type(
                    typed,
                    capture.ty(),
                    function.instance.substitutions(),
                    capture.reference_span(),
                )?;
                if builtin_type(typed, concrete) == Some(BuiltinType::Unit) {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.reference_span(),
                    ));
                }
                let ty = types.intern(module, typed, concrete, capture.reference_span())?;
                environment_fields.push(ty);
                capture_types.push(ClosureCaptureType {
                    mode: SsaCaptureMode::Owned,
                    ty,
                });
                captures.push(CapturePlan {
                    symbol,
                    ty,
                    effect: capture.effect(),
                    span: capture.reference_span(),
                });
            }
            let identity = format!(
                "unit.lambda.f{}.s{}.e{}",
                function.id.index(),
                function.instance.source_unit().index(),
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
            let thunk = module
                .add_function(
                    format!("{identity}.thunk"),
                    callable_returns,
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            module
                .function_mut(thunk)
                .expect("new unit closure thunk exists")
                .add_block(thunk_parameters, Origin::Source(span))
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            if closures
                .insert(
                    (function.id, id),
                    CallablePlan {
                        scope: function.id,
                        source_unit: function.instance.source_unit(),
                        expression: id,
                        callable,
                        thunk,
                        body: *body,
                        span,
                        parameter_spans: parameters.clone(),
                        return_type,
                        result_expression,
                        captures,
                        substitutions: function.instance.substitutions().clone(),
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

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_callable_literal(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if !self.closure_binding_context {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let plan = self
            .callable_plans
            .get(&(
                self.closure_scope,
                UnitExpressionId::new(self.source_unit, expression),
            ))
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let mut captures = Vec::with_capacity(plan.captures.len());
        for capture in &plan.captures {
            let value = match capture.effect {
                ClosureCaptureEffect::Copy => {
                    if let Some(LoweredValue::Value(value)) =
                        self.bindings.get(&capture.symbol).copied()
                    {
                        value
                    } else if let Some(loan) = self.borrow_bindings.get(&capture.symbol).copied() {
                        let (_, results) = self
                            .function
                            .append_instruction(
                                self.block,
                                Operation::Read {
                                    source: crate::ssa::model::PlaceAccess::Loan(loan),
                                },
                                vec![EntityType::Value(capture.ty)],
                                Origin::Source(capture.span),
                            )
                            .map_err(|_| {
                                lowering_error(LoweringErrorKind::InvalidModel, capture.span)
                            })?;
                        require_value(results[0], capture.span)?
                    } else {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, capture.span));
                    }
                }
                ClosureCaptureEffect::Move => {
                    let Some(LoweredValue::Value(value)) =
                        self.bindings.get(&capture.symbol).copied()
                    else {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, capture.span));
                    };
                    self.take_owned_binding(capture.symbol, value, capture.span)?;
                    value
                }
                ClosureCaptureEffect::Borrow | ClosureCaptureEffect::Unknown => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.span,
                    ));
                }
            };
            captures.push(ClosureCaptureOperand::Owned(value));
        }
        if captures.is_empty() {
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::FunctionAddress { target: plan.thunk },
                    vec![EntityType::Value(plan.callable)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            return Ok(LoweredValue::Value(require_value(results[0], span)?));
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::ClosureConstruct {
                    closure: plan.callable,
                    thunk: plan.thunk,
                    captures,
                },
                vec![EntityType::Value(plan.callable)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    pub(super) fn lower_function_value_call(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let callee_node = self
            .parsed
            .ast()
            .expressions()
            .get(callee)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let return_type = resolve_concrete_type(
            self.typed,
            descriptor.return_type(),
            self.substitutions,
            span,
        )?;
        if !matches!(callee_node.payload(), Expression::Name) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let callable = match self.lower_expression(callee)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Unit | LoweredValue::Diverged => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        };
        let callee_id = UnitExpressionId::new(self.source_unit, callee);
        let call = UnitExpressionId::new(self.source_unit, expression);
        let Some(LoweredCallArguments {
            arguments,
            created_loans,
        }) = self.lower_call_arguments(call, arguments, descriptor, span)?
        else {
            return Ok(LoweredValue::Diverged);
        };
        let result_types = if builtin_type(self.typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![EntityType::Value(
                *self
                    .type_ids
                    .get(&return_type)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
            )]
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::CallableInvoke {
                    callable,
                    arguments,
                },
                result_types,
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for (loan, end_span) in created_loans.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(end_span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
        }
        self.emit_drops(
            lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(callee_id),
        )?;
        self.emit_drops(lang_frontend::ownership_checking::UnitDropPoint::CallReturn(call))?;
        match results.as_slice() {
            [] => Ok(LoweredValue::Unit),
            [result] => Ok(LoweredValue::Value(require_value(*result, span)?)),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
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
            Expression::Name => Ok(self
                .references
                .get(&span_key(node.span()))
                .and_then(|symbol| self.closure_bindings.get(symbol))
                .copied()),
            _ => Ok(None),
        }
    }

    pub(super) fn bind_callable_entry(&mut self, plan: &CallablePlan) -> Result<(), LoweringError> {
        let parameters = self
            .function
            .block(self.block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, plan.span))?
            .parameters
            .clone();
        let parameter_offset = usize::from(!plan.captures.is_empty());
        if parameters.len() != plan.parameter_spans.len() + parameter_offset {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, plan.span));
        }
        if let Some(first) = parameters.first().filter(|_| !plan.captures.is_empty()) {
            let EntityId::Loan(environment) = first else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, plan.span));
            };
            for (field, capture) in plan.captures.iter().enumerate() {
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::SharedFieldLoan {
                            base: *environment,
                            field,
                        },
                        vec![EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: capture.ty,
                        }],
                        Origin::Source(capture.span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, capture.span))?;
                let EntityId::Loan(loan) = results[0] else {
                    return Err(lowering_error(
                        LoweringErrorKind::InvalidModel,
                        capture.span,
                    ));
                };
                self.borrow_bindings.insert(capture.symbol, loan);
            }
        }
        for (span, entity) in plan
            .parameter_spans
            .iter()
            .zip(parameters.into_iter().skip(parameter_offset))
        {
            let symbol = self.declaration_symbol(*span, SymbolKind::LambdaParameter)?;
            match entity {
                EntityId::Value(value) => {
                    self.bindings.insert(symbol, LoweredValue::Value(value));
                }
                EntityId::Loan(loan) => {
                    self.borrow_bindings.insert(symbol, loan);
                }
                EntityId::Place(_) => {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, *span));
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_closure_drop_facts(
        &self,
        facts: &[UnitDropFact],
    ) -> Result<(), LoweringError> {
        for fact in facts {
            let UnitDropTarget::Captured { closure, .. } = fact.target() else {
                continue;
            };
            if !facts.iter().any(|candidate| {
                matches!(candidate.target(), UnitDropTarget::Named(symbol)
                    if self.closure_bindings.get(&symbol) == Some(&closure))
            }) {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    fact.value_origin(),
                ));
            }
        }
        for fact in facts {
            let UnitDropTarget::Named(symbol) = fact.target() else {
                continue;
            };
            let Some(closure) = self.closure_bindings.get(&symbol).copied() else {
                continue;
            };
            let plan = self
                .callable_plans
                .get(&(self.closure_scope, closure))
                .ok_or_else(|| {
                    lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
                })?;
            let expected = plan
                .captures
                .iter()
                .rev()
                .filter(|capture| capture.effect == ClosureCaptureEffect::Move)
                .map(|capture| UnitClosureCaptureSource::Symbol(capture.symbol))
                .collect::<Vec<_>>();
            let actual = facts
                .iter()
                .filter_map(|candidate| match candidate.target() {
                    UnitDropTarget::Captured {
                        closure: candidate,
                        source,
                    } if candidate == closure => Some(source),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if actual != expected {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    fact.value_origin(),
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn finish_thunk(
    lowerer: &mut UnitExpressionLowerer<'_>,
    plan: &CallablePlan,
) -> Result<(), LoweringError> {
    lowerer.bind_callable_entry(plan)?;
    lowerer.emit_drops(
        lang_frontend::ownership_checking::UnitDropPoint::LambdaEntry(plan.expression),
    )?;
    let result = if builtin_type(lowerer.typed, plan.return_type) == Some(BuiltinType::Unit) {
        lowerer.lower_statement(plan.body)?
    } else {
        lowerer.lower_tail_value_body(plan.body)?
    };
    if let LoweredValue::Value(value) = result {
        let expression = plan
            .result_expression
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, plan.span))?;
        lowerer.transfer_owned_expression(expression, value, plan.span)?;
    }
    if !lowerer.temporaries.is_empty() {
        return Err(lowering_error(
            LoweringErrorKind::UnsupportedNode,
            plan.span,
        ));
    }
    for symbol in lowerer.bindings.keys() {
        let ty = lowerer
            .typed
            .types()
            .body_symbol_types()
            .get(symbol)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, plan.span))?;
        let ty = resolve_concrete_type(lowerer.typed, ty, &plan.substitutions, plan.span)?;
        if lowerer.typed.types().copyability(ty) == Copyability::MoveOnly {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                plan.span,
            ));
        }
    }
    if result == LoweredValue::Diverged {
        return Ok(());
    }
    let values = match (builtin_type(lowerer.typed, plan.return_type), result) {
        (Some(BuiltinType::Unit), LoweredValue::Unit) => Vec::new(),
        (Some(BuiltinType::Unit), LoweredValue::Value(_))
        | (_, LoweredValue::Unit | LoweredValue::Diverged) => {
            return Err(lowering_error(LoweringErrorKind::MissingFact, plan.span));
        }
        (_, LoweredValue::Value(value)) => vec![value],
    };
    lowerer
        .function
        .set_terminator(
            lowerer.block,
            TerminatorKind::Return { values },
            Origin::Source(plan.span),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, plan.span))
}

fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

fn strictly_contains(owner: Span, child: Span) -> bool {
    span_contains(owner, child) && (owner.start() < child.start() || child.end() < owner.end())
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
