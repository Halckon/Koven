//! Per-source callable lookup; frontend Function identity never selects an environment globally.

use super::*;
use crate::ssa::lowering_support::callable_instances::{CallableKey, CallableToken};
use lang_frontend::ownership_checking::CallableOrigin;

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
