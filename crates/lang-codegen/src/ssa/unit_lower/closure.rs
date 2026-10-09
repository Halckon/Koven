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

#[cfg(test)]
mod generic_runtime_tests;
mod known_function;
pub(super) mod layout;
#[cfg(test)]
mod runtime_slots_tests;
#[cfg(test)]
mod runtime_tests;
mod shared;

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
    body: StatementId,
    span: Span,
    parameter_spans: Vec<Span>,
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
    pub(super) thunk: FunctionId,
    pub(super) body: StatementId,
    pub(super) span: Span,
    pub(super) parameter_spans: Vec<Span>,
    pub(super) return_type: UnitTypeId,
    pub(super) result_expression: Option<ExpressionId>,
    pub(super) captures: Vec<CapturePlan>,
    pub(super) substitutions: BTreeMap<UnitSymbolId, UnitTypeId>,
    pub(super) static_self: Option<UnitTypeId>,
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
        let thunk = module
            .add_function(
                format!("{}.thunk", layout.identity),
                layout.returns,
                Origin::Source(layout.span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, layout.span))?;
        module
            .function_mut(thunk)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, layout.span))?
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
                thunk,
                body: layout.body,
                span: layout.span,
                parameter_spans: layout.parameter_spans,
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
                self.closure_scope,
                UnitExpressionId::new(self.source_unit, expression),
            ))
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        if !self.closure_binding_context && !plan.captures.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut captures = Vec::with_capacity(plan.captures.len());
        for (field, capture) in plan.captures.iter().enumerate() {
            if capture.mode == ClosureCaptureMode::Shared {
                let loan = self.form_shared_capture(plan.expression, field, capture)?;
                captures.push(ClosureCaptureOperand::Shared(loan));
                continue;
            }
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
            self.static_self,
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
        let callable_index = self.pending_operands.len();
        self.pending_operands.push(EntityId::Value(callable));
        // callee 也属于调用前缀；参数退出时须先移除其 pending alias，再消费 owner drop。
        self.pending_call_frames
            .push(super::call_lifetimes::PendingCallFrame {
                call,
                receiver: false,
                loan_arguments: Vec::new(),
                loop_depth: self.loops.len(),
                pending_start: callable_index,
                exclusive_root_owners: Vec::new(),
                field_replace_owner: None,
                shared_field_roots: Vec::new(),
                created_loans: Vec::new(),
                abi_slots: Vec::new(),
            });
        let lowered_arguments = self.lower_call_arguments(call, arguments, descriptor, span);
        self.pending_call_frames.pop();
        let Some(LoweredCallArguments {
            arguments,
            created_loans,
            abi_owners,
        }) = lowered_arguments?
        else {
            self.pending_operands.truncate(callable_index);
            return Ok(LoweredValue::Diverged);
        };
        let callable = require_value(self.pending_operands[callable_index], span)?;
        self.pending_operands.truncate(callable_index);
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
        self.drop_abi_call_owners(abi_owners, span)?;
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
                            target: capture.storage,
                        }],
                        Origin::Source(capture.span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, capture.span))?;
                let EntityId::Loan(mut loan) = results[0] else {
                    return Err(lowering_error(
                        LoweringErrorKind::InvalidModel,
                        capture.span,
                    ));
                };
                if capture.mode == ClosureCaptureMode::Shared {
                    self.capture_loans.insert((plan.expression, field), loan);
                    let (_, results) = self
                        .function
                        .append_instruction(
                            self.block,
                            Operation::SharedReferenceFollow { source: loan },
                            vec![EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: capture.ty,
                            }],
                            Origin::Source(capture.span),
                        )
                        .map_err(|_| {
                            lowering_error(LoweringErrorKind::InvalidModel, capture.span)
                        })?;
                    let EntityId::Loan(target) = results[0] else {
                        return Err(lowering_error(
                            LoweringErrorKind::InvalidModel,
                            capture.span,
                        ));
                    };
                    loan = target;
                }
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
                    || matches!(candidate.target(), UnitDropTarget::Temporary(expression)
                        if self.closure_origin(expression.expression()).ok().flatten() == Some(closure))
            }) {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    fact.value_origin(),
                ));
            }
        }
        for fact in facts {
            let closure = match fact.target() {
                UnitDropTarget::Named(symbol) => self.closure_bindings.get(&symbol).copied(),
                UnitDropTarget::Temporary(expression) => {
                    self.closure_origin(expression.expression())?
                }
                _ => None,
            };
            let Some(closure) = closure else { continue };
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
    lowerer.thunk_expression = Some(plan.expression);
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
    // Abort does not unwind its operand-prefix owners; ordinary Return already
    // consumed its frontend cleanup facts and is checked by the SSA verifier.
    if result == LoweredValue::Diverged {
        return Ok(());
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
            .body_symbol_types()
            .get(symbol)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, plan.span))?;
        let ty = resolve_concrete_type(
            lowerer.typed,
            ty,
            &plan.substitutions,
            plan.static_self,
            plan.span,
        )?;
        if lowerer.typed.copyability(ty) == Copyability::MoveOnly {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                plan.span,
            ));
        }
    }
    lowerer.end_thunk_capture_views(plan.span)?;
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
