//! Source-qualified concrete callable layouts, capture formation, and per-source lookup.

mod formation;

use super::instances::{FunctionInstancePlan, resolve_concrete_type};
use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
    nominal::NominalTypeMapper, value,
};
use crate::ssa::{
    lowering_support::callable_instances::{CallableKey, CallableToken, SourceToken},
    model::{
        ClosureCaptureMode as SsaCaptureMode, ClosureCaptureType, EntityId, EntityType, FunctionId,
        LoanKind, Module, Operation, Origin, SsaTypeId,
    },
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{NameResolution, SymbolId, SymbolKind},
    ownership_checking::{
        CallableOrigin, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
        OwnershipCheckedFile,
    },
    parser::{Expression, ParsedFile, Statement},
    source::Span,
    type_checking::{BuiltinType, ParameterMode, TypeId, TypeKind, TypedFile},
};
use std::collections::BTreeMap;

pub(in crate::ssa::lower_frontend) struct Inputs<'a> {
    pub(in crate::ssa::lower_frontend) parsed: &'a ParsedFile,
    pub(in crate::ssa::lower_frontend) names: &'a NameResolution,
    pub(in crate::ssa::lower_frontend) typed: &'a TypedFile,
    pub(in crate::ssa::lower_frontend) owned: &'a OwnershipCheckedFile,
    pub(in crate::ssa::lower_frontend) instances: &'a FunctionInstancePlan,
    pub(in crate::ssa::lower_frontend) templates: &'a [super::instances::FunctionTemplate],
}

pub(in crate::ssa::lower_frontend) fn declare(
    module: &mut Module,
    mapper: &mut NominalTypeMapper,
    inputs: Inputs<'_>,
) -> Result<CallableLayouts, LoweringError> {
    let Inputs {
        parsed,
        names,
        typed,
        owned,
        instances,
        templates,
    } = inputs;
    let mut layouts = CallableLayouts::default();
    for (ordinal, instance) in instances.instances().iter().enumerate() {
        let owner_span = templates[instance.template_index].span;
        let lambdas = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(expression, node)| {
                (contains(owner_span, node.span())
                    && matches!(node.payload(), Expression::Lambda { .. })
                    && owned.callable_origin(expression).is_some_and(|fact| {
                        fact.origin() == CallableOrigin::Lambda(expression)
                    }))
                .then_some((expression, node.span()))
            })
            .collect::<Vec<_>>();
        for &(expression, span) in &lambdas {
            if lambdas
                .iter()
                .any(|&(other, outer)| other != expression && contains(outer, span))
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let node = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Lambda {
                opener_span,
                parameters,
                arrow_span,
                body,
                ..
            } = node.payload()
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            owned
                .closure(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let ty = typed
                .expression_type(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let ty = resolve_concrete_type(typed, ty, &instance.substitutions, span)?;
            let Some(TypeKind::Function {
                parameters: signature,
                return_type,
                ..
            }) = typed.types().get(ty)
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let parameter_spans =
                if arrow_span.is_none() && parameters.is_empty() && signature.len() == 1 {
                    std::slice::from_ref(opener_span)
                } else {
                    parameters.as_slice()
                };
            if parameter_spans.len() != signature.len() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let mut symbols = Vec::new();
            let mut parameter_types = Vec::new();
            for (parameter, &parameter_span) in signature.iter().zip(parameter_spans) {
                let symbol = names
                    .symbols()
                    .iter()
                    .find(|symbol| {
                        symbol.span() == parameter_span
                            && symbol.kind() == SymbolKind::LambdaParameter
                    })
                    .map(|symbol| symbol.id())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, parameter_span))?;
                symbols.push(symbol);
                let target = mapper.intern(module, names, typed, parameter.ty, parameter_span)?;
                parameter_types.push(match parameter.mode {
                    ParameterMode::Value => EntityType::Value(target),
                    ParameterMode::Borrow => EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    },
                    ParameterMode::Inout => {
                        return Err(error(LoweringErrorKind::UnsupportedNode, parameter_span));
                    }
                });
            }
            let returns = if builtin_type(typed, *return_type) == Some(BuiltinType::Unit) {
                Vec::new()
            } else {
                vec![mapper.intern(module, names, typed, *return_type, span)?]
            };
            let mut captures = Vec::new();
            let mut fields = Vec::new();
            for capture in owned.captures_of(expression) {
                let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        capture.reference_span(),
                    ));
                };
                let concrete = resolve_concrete_type(
                    typed,
                    capture.ty(),
                    &instance.substitutions,
                    capture.reference_span(),
                )?;
                let target =
                    mapper.intern(module, names, typed, concrete, capture.reference_span())?;
                let mode = match capture.mode() {
                    ClosureCaptureMode::Owned => SsaCaptureMode::Owned,
                    ClosureCaptureMode::Shared => SsaCaptureMode::Shared,
                };
                fields.push(if mode == SsaCaptureMode::Shared {
                    module
                        .add_shared_reference_type(target)
                        .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?
                } else {
                    target
                });
                captures.push(CapturePlan {
                    symbol,
                    ty: target,
                    field_type: *fields
                        .last()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                    mode,
                    effect: capture.effect(),
                    span: capture.reference_span(),
                });
            }
            let name = format!("lambda{}.s{}", expression.index(), ordinal);
            let environment = if captures.is_empty() {
                None
            } else {
                Some(
                    module
                        .add_aggregate_type(format!("{name}.environment"), fields)
                        .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?,
                )
            };
            let callable = if let Some(environment) = environment {
                module.add_concrete_closure_type_with_parameters(
                    format!("{name}.closure"),
                    parameter_types.clone(),
                    returns.clone(),
                    environment,
                    captures
                        .iter()
                        .map(|capture| ClosureCaptureType {
                            mode: capture.mode,
                            ty: capture.ty,
                        })
                        .collect(),
                )
            } else {
                module.add_function_pointer_type_with_parameters(
                    parameter_types.clone(),
                    returns.clone(),
                )
            }
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let thunk = module
                .add_function(format!("{name}.thunk"), returns, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            if let Some(target) = environment {
                parameter_types.insert(
                    0,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    },
                );
            }
            module
                .function_mut(thunk)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .add_block(parameter_types, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let result_expression = tail(parsed, *body, span)?;
            layouts.lambdas.insert(
                (instance.source, expression.index()),
                ClosurePlan {
                    owner: instance.source,
                    expression,
                    callable,
                    thunk,
                    body: *body,
                    span,
                    captures,
                    parameter_symbols: symbols,
                    return_type: *return_type,
                    result_expression,
                    substitutions: instance.substitutions.clone(),
                },
            );
        }
    }
    Ok(layouts)
}

fn tail(
    parsed: &ParsedFile,
    body: StatementId,
    span: Span,
) -> Result<Option<ExpressionId>, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
    let Statement::LambdaBody { elements } = node.payload() else {
        return Err(error(LoweringErrorKind::MissingFact, span));
    };
    elements
        .last()
        .map(|statement| {
            parsed
                .ast()
                .statements()
                .get(*statement)
                .map(|node| match node.payload() {
                    Statement::Expression { expression } => Some(*expression),
                    _ => None,
                })
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))
        })
        .transpose()
        .map(Option::flatten)
}

fn contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

#[derive(Clone, Copy)]
pub(super) struct CapturePlan {
    pub(super) symbol: SymbolId,
    pub(super) ty: SsaTypeId,
    pub(super) field_type: SsaTypeId,
    pub(super) mode: SsaCaptureMode,
    pub(super) effect: ClosureCaptureEffect,
    pub(super) span: Span,
}

#[derive(Clone)]
pub(super) struct ClosurePlan {
    pub(super) owner: SourceToken,
    pub(super) expression: ExpressionId,
    pub(super) callable: SsaTypeId,
    pub(super) thunk: FunctionId,
    pub(super) body: StatementId,
    pub(super) span: Span,
    pub(super) captures: Vec<CapturePlan>,
    pub(super) parameter_symbols: Vec<SymbolId>,
    pub(super) return_type: TypeId,
    pub(super) result_expression: Option<ExpressionId>,
    pub(super) substitutions: BTreeMap<SymbolId, TypeId>,
}

#[derive(Default)]
pub(super) struct CallableLayouts {
    pub(super) lambdas: BTreeMap<(SourceToken, usize), ClosurePlan>,
    pub(super) named: BTreeMap<SourceToken, SsaTypeId>,
}

impl CallableLayouts {
    pub(in crate::ssa::lower_frontend) fn token_type(
        &self,
        instances: &FunctionInstancePlan,
        token: CallableToken,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        match instances
            .callable(token)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
        {
            CallableKey::Lambda { owner, expression } => self
                .lambdas
                .get(&(*owner, expression.expression().index()))
                .map(|plan| plan.callable),
            CallableKey::KnownFunction { function } => self.named.get(function).copied(),
        }
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    pub(in crate::ssa::lower_frontend) fn expression_type(
        &self,
        lowerer: &ExpressionLowerer<'_>,
        expression: ExpressionId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let parsed = lowerer.parsed;
        let typed = lowerer.typed;
        let owned = lowerer.owned;
        let instances = lowerer.instance_plan;
        let source = lowerer
            .source_token
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Lambda { .. } = node.payload() {
            return self
                .lambdas
                .get(&(source, expression.index()))
                .map(|plan| plan.callable)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span));
        }
        if let Expression::Group { expression } = node.payload() {
            return self.expression_type(lowerer, *expression, span);
        }
        let origin = owned
            .callable_origin(expression)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?
            .origin();
        match origin {
            CallableOrigin::Lambda(lambda) => self
                .lambdas
                .get(&(source, lambda.index()))
                .map(|plan| plan.callable)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
            CallableOrigin::Parameter(parameter) => {
                let key = instances
                    .source(source)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let signature = typed
                    .callables()
                    .iter()
                    .find(|signature| signature.symbol() == key.symbol())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = signature
                    .parameter_symbols()
                    .iter()
                    .position(|symbol| *symbol == Some(parameter))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let token = key
                    .callable_arguments()
                    .iter()
                    .find(|(index, _)| *index == slot)
                    .map(|(_, token)| *token)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.token_type(instances, token, span)
            }
            CallableOrigin::KnownFunction(function) => {
                let instance = instances
                    .instances()
                    .iter()
                    .find(|instance| {
                        instance.key.symbol() == function
                            && instance.key.type_arguments().is_empty()
                            && instance.key.callable_arguments().is_empty()
                    })
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.named
                    .get(&instance.source)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            }
            CallableOrigin::FactoryResult(call) => {
                let key = instances
                    .call_site(source, call)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let factory = instances
                    .instances()
                    .iter()
                    .find(|instance| &instance.key == key)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let token = instances
                    .pointer_return(factory.source)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                self.token_type(instances, token, span)
            }
        }
    }
}

#[cfg(test)]
mod tests;
