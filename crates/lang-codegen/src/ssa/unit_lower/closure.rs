//! compilation-unit lambda 的 function pointer/concrete closure、thunk 与调用 lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, UnitClosureCaptureSource, UnitDropFact,
        UnitDropTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::{BuiltinType, UnitCallDescriptor, UnitExpressionId, UnitTypeId, UnitTypeKind},
};

use super::{
    FunctionPlan, LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value,
    resolve_concrete_type, span_key, type_lower::UnitTypeLowering,
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
    pub(super) callable: SsaTypeId,
    pub(super) thunk: FunctionId,
    pub(super) body: StatementId,
    pub(super) span: Span,
    pub(super) captures: Vec<CapturePlan>,
    pub(super) substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
}

pub(super) fn declare(
    module: &mut Module,
    parsed_by_source: &[&lang_frontend::parser::ParsedFile],
    plans: &[FunctionPlan],
    typed: &lang_frontend::type_checking::ValidatedCompilationUnitTypes,
    owned: &lang_frontend::ownership_checking::ValidatedCompilationUnitOwnership,
    types: &mut UnitTypeLowering,
) -> Result<BTreeMap<CallablePlanKey, CallablePlan>, LoweringError> {
    let mut closures = BTreeMap::new();
    for function in plans {
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
            if !parameters.is_empty()
                || !callable_parameters.is_empty()
                || builtin_type(typed, return_type) != Some(BuiltinType::Unit)
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            for (&candidate, &candidate_type) in typed.types().expression_types() {
                if candidate.source_unit() != function.instance.source_unit() {
                    continue;
                }
                let candidate_span = parsed
                    .ast()
                    .expressions()
                    .get(candidate.expression())
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                    .span();
                if !strictly_contains(span, candidate_span) {
                    continue;
                }
                let candidate_type = resolve_concrete_type(
                    typed,
                    candidate_type,
                    function.instance.substitutions(),
                    candidate_span,
                )?;
                if typed.types().expression_category(candidate)
                    == Some(lang_frontend::type_checking::ExpressionCategory::Temporary)
                    && typed.types().copyability(candidate_type)
                        == lang_frontend::type_checking::Copyability::MoveOnly
                    && super::type_lower::is_supported_storage_type(typed, candidate_type)
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        candidate_span,
                    ));
                }
            }

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
                    .add_function_pointer_type(Vec::new(), Vec::new())
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                (callable, Vec::new())
            } else {
                let environment = module
                    .add_aggregate_type(format!("{identity}.environment"), environment_fields)
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let callable = module
                    .add_concrete_closure_type(
                        format!("{identity}.closure"),
                        Vec::new(),
                        Vec::new(),
                        environment,
                        capture_types,
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                (
                    callable,
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: environment,
                    }],
                )
            };
            let thunk = module
                .add_function(
                    format!("{identity}.thunk"),
                    Vec::new(),
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
                        callable,
                        thunk,
                        body: *body,
                        span,
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
        if !arguments.is_empty()
            || !descriptor.arguments().is_empty()
            || builtin_type(self.typed, descriptor.return_type()) != Some(BuiltinType::Unit)
            || !matches!(callee_node.payload(), Expression::Name)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let callable = match self.lower_expression(callee)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Unit | LoweredValue::Diverged => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        };
        let callee_id = UnitExpressionId::new(self.source_unit, callee);
        self.function
            .append_instruction(
                self.block,
                Operation::CallableInvoke {
                    callable,
                    arguments: Vec::new(),
                },
                Vec::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.emit_drops(
            lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(callee_id),
        )?;
        self.emit_drops(
            lang_frontend::ownership_checking::UnitDropPoint::CallReturn(UnitExpressionId::new(
                self.source_unit,
                expression,
            )),
        )?;
        Ok(LoweredValue::Unit)
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

    pub(super) fn bind_capture_views(&mut self, plan: &CallablePlan) -> Result<(), LoweringError> {
        if plan.captures.is_empty() {
            let parameters = &self
                .function
                .block(self.block)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, plan.span))?
                .parameters;
            return if parameters.is_empty() {
                Ok(())
            } else {
                Err(lowering_error(LoweringErrorKind::InvalidModel, plan.span))
            };
        }
        let [EntityId::Loan(environment)] = self
            .function
            .block(self.block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, plan.span))?
            .parameters
            .as_slice()
        else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, plan.span));
        };
        let environment = *environment;
        for (field, capture) in plan.captures.iter().enumerate() {
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::SharedFieldLoan {
                        base: environment,
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
    lowerer.bind_capture_views(plan)?;
    let result = lowerer.lower_statement(plan.body)?;
    if result == LoweredValue::Diverged {
        return Ok(());
    }
    if result != LoweredValue::Unit {
        return Err(lowering_error(LoweringErrorKind::MissingFact, plan.span));
    }
    lowerer
        .function
        .set_terminator(
            lowerer.block,
            TerminatorKind::Return { values: Vec::new() },
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
