//! Concrete callable types are queried by sealed source provenance, never semantic Fn TypeId.

use super::*;
use crate::ssa::lowering_support::callable_instances::{CallableKey, CallableToken, SourceToken};
use lang_frontend::ownership_checking::UnitCallableOrigin;

pub(super) struct CallableAbi {
    lambdas: BTreeMap<(SourceToken, UnitExpressionId), SsaTypeId>,
    named: BTreeMap<SourceToken, SsaTypeId>,
    sources: BTreeMap<UnitFunctionInstanceKey, SourceToken>,
}

impl CallableAbi {
    pub(super) fn declare(
        module: &mut crate::ssa::model::Module,
        instances: &[UnitPlannedInstance],
        plan: &crate::ssa::unit_plan::UnitCallablePlan,
        layouts: &closure::CallableLayouts,
        typed: &CompilationUnitTypes,
        types: &mut type_lower::UnitTypeLowering,
    ) -> Result<Self, LoweringError> {
        let sources = instances
            .iter()
            .map(|instance| (instance.key().clone(), instance.source_token()))
            .collect();
        let mut named_sources = std::collections::BTreeSet::new();
        for instance in instances {
            let runtime_tokens = typed
                .container_constructions()
                .iter()
                .map(|d| d.expression())
                .chain(typed.map_with_values().iter().map(|d| d.expression()))
                .filter_map(|call| plan.runtime_callback(instance.source_token(), call));
            for token in instance
                .key()
                .callable_arguments()
                .iter()
                .map(|(_, token)| *token)
                .chain(runtime_tokens)
                .chain(
                    plan.callable_return(instance.source_token())
                        .map(|value| value.callable()),
                )
            {
                if let Some(CallableKey::KnownFunction { function }) = plan.callable(token) {
                    named_sources.insert(*function);
                }
            }
        }
        let mut named = BTreeMap::new();
        for source in named_sources {
            let key = plan.source(source).ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
            let instance = instances
                .iter()
                .find(|instance| instance.key() == key)
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            let span = instance.span();
            let signature = unit_callable_signature(typed, key.target())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let mut parameters = Vec::new();
            for parameter in signature.parameters() {
                let ty = resolve_concrete_type(
                    typed,
                    parameter.ty(),
                    instance.substitutions(),
                    key.static_self(),
                    parameter.span(),
                )?;
                let ty = types.intern(module, typed, ty, parameter.span())?;
                parameters.push(match parameter.mode() {
                    ParameterMode::Value => EntityType::Value(ty),
                    ParameterMode::Borrow => EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    },
                    ParameterMode::Inout => {
                        return Err(lowering_error(
                            LoweringErrorKind::UnsupportedNode,
                            parameter.span(),
                        ));
                    }
                });
            }
            let result = resolve_concrete_type(
                typed,
                signature.return_type(),
                instance.substitutions(),
                key.static_self(),
                span,
            )?;
            let returns = if builtin_type(typed, result) == Some(BuiltinType::Unit) {
                Vec::new()
            } else {
                vec![types.intern(module, typed, result, span)?]
            };
            let callable = module
                .add_function_pointer_type_with_parameters(parameters, returns)
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            named.insert(source, callable);
        }
        Ok(Self {
            lambdas: layouts
                .iter()
                .map(|(key, layout)| (*key, layout.callable))
                .collect(),
            named,
            sources,
        })
    }

    pub(super) fn token_type(
        &self,
        plan: &crate::ssa::unit_plan::UnitCallablePlan,
        token: CallableToken,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let ty = match plan.callable(token) {
            Some(CallableKey::Lambda { owner, expression }) => {
                self.lambdas.get(&(*owner, *expression))
            }
            Some(CallableKey::KnownFunction { function }) => self.named.get(function),
            None => None,
        };
        ty.copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    pub(super) fn source_token(
        &self,
        key: &UnitFunctionInstanceKey,
        span: Span,
    ) -> Result<SourceToken, LoweringError> {
        self.sources
            .get(key)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    pub(super) fn parameter_type(
        &self,
        plan: &crate::ssa::unit_plan::UnitCallablePlan,
        key: &UnitFunctionInstanceKey,
        slot: usize,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let token = key
            .callable_arguments()
            .iter()
            .find(|(index, _)| *index == slot)
            .map(|(_, token)| *token)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.token_type(plan, token, span)
    }

    pub(super) fn return_type(
        &self,
        plan: &crate::ssa::unit_plan::UnitCallablePlan,
        source: SourceToken,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let returned = plan
            .callable_return(source)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        self.token_type(plan, returned.callable(), span)
    }

    pub(super) fn expression_type(
        &self,
        plan: &crate::ssa::unit_plan::UnitCallablePlan,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        source: SourceToken,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        if owned.closure(expression).is_some() {
            return self
                .lambdas
                .get(&(source, expression))
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let fact = owned
            .callable_origin(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        match fact.origin() {
            UnitCallableOrigin::Lambda(lambda) => self
                .lambdas
                .get(&(source, lambda))
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
            UnitCallableOrigin::Parameter(symbol) => {
                let key = plan
                    .source(source)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let signature = unit_callable_signature(typed, key.target())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let slot = signature
                    .parameters()
                    .iter()
                    .position(|parameter| parameter.symbol() == Some(symbol))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
                self.parameter_type(plan, key, slot, span)
            }
            UnitCallableOrigin::KnownFunction(target) => {
                let key = UnitFunctionInstanceKey::for_target(target, Vec::new());
                let source = self.source_token(&key, span)?;
                self.named
                    .get(&source)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
            }
            UnitCallableOrigin::FactoryResult(call) => {
                let route = plan
                    .call_site(source, call)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                self.return_type(plan, self.source_token(route.key(), span)?, span)
            }
        }
    }
}
