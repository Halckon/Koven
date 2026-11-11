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

#[cfg(test)]
mod generic_runtime_tests;
pub(super) mod layout;
#[cfg(test)]
mod runtime_slots_tests;
#[cfg(test)]
mod runtime_tests;

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
