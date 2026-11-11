//! Concrete callable types are queried by sealed source provenance, never semantic Fn TypeId.

use super::*;
use crate::ssa::lowering_support::callable_instances::{CallableKey, CallableToken, SourceToken};
use lang_frontend::ownership_checking::UnitCallableOrigin;

pub(super) struct CallableAbi {
    lambdas: BTreeMap<(SourceToken, UnitExpressionId), SsaTypeId>,
    named: BTreeMap<SourceToken, SsaTypeId>,
    sources: BTreeMap<UnitFunctionInstanceKey, SourceToken>,
    /// ADR-0034 view targets of Borrow function parameters, by concrete function type.
    views: BTreeMap<UnitTypeId, SsaTypeId>,
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
                .chain(plan.borrowed_arguments(instance.source_token()))
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
            let result_builtin = builtin_type(typed, result);
            let returns = if matches!(
                result_builtin,
                Some(BuiltinType::Unit | BuiltinType::Nothing)
            ) {
                Vec::new()
            } else {
                vec![types.intern(module, typed, result, span)?]
            };
            let callable = module
                .add_function_pointer_signature(crate::ssa::model::CallableSignature {
                    parameters,
                    returns,
                    never_returns: result_builtin == Some(BuiltinType::Nothing),
                })
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
            views: BTreeMap::new(),
        })
    }

    pub(super) fn record_view(&mut self, function: UnitTypeId, view: SsaTypeId) {
        self.views.insert(function, view);
    }

    pub(super) fn views(&self) -> &BTreeMap<UnitTypeId, SsaTypeId> {
        &self.views
    }

    pub(super) fn known_functions(
        &self,
        function_ids: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
        plan: &crate::ssa::unit_plan::UnitCallablePlan,
    ) -> BTreeMap<lang_frontend::type_checking::UnitCallableTarget, (FunctionId, SsaTypeId)> {
        let mut known = BTreeMap::new();
        for (&source, &callable) in &self.named {
            if let Some(key) = plan.source(source)
                && key.type_arguments().is_empty()
                && key.callable_arguments().is_empty()
                && let Some(&function) = function_ids.get(key)
            {
                known.insert(key.target(), (function, callable));
            }
        }
        known
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
            UnitCallableOrigin::Parameter(_) => {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
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

/// ADR-0034: the callable view loan target for a concrete function type. Borrow function
/// parameters receive this view instead of a callable-provenance specialization.
pub(super) fn view_type(
    module: &mut crate::ssa::model::Module,
    typed: &CompilationUnitTypes,
    types: &mut type_lower::UnitTypeLowering,
    function: UnitTypeId,
    span: Span,
) -> Result<SsaTypeId, LoweringError> {
    let Some(UnitTypeKind::Function {
        parameters,
        return_type,
        ..
    }) = typed.types().get(function).cloned()
    else {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    };
    let is_function = |ty| matches!(typed.types().get(ty), Some(UnitTypeKind::Function { .. }));
    let mut entities = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        if is_function(parameter.ty()) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let ty = types.intern(module, typed, parameter.ty(), span)?;
        entities.push(match parameter.mode() {
            ParameterMode::Value => EntityType::Value(ty),
            ParameterMode::Borrow => EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            },
            ParameterMode::Inout => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        });
    }
    let result_builtin = builtin_type(typed, return_type);
    let returns = if matches!(
        result_builtin,
        Some(BuiltinType::Unit | BuiltinType::Nothing)
    ) {
        Vec::new()
    } else if is_function(return_type) {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    } else {
        vec![types.intern(module, typed, return_type, span)?]
    };
    module
        .add_callable_view_type(crate::ssa::model::CallableSignature {
            parameters: entities,
            returns,
            never_returns: result_builtin == Some(BuiltinType::Nothing),
        })
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
}

pub(super) fn declare_deinit(
    module: &mut crate::ssa::model::Module,
    parsed_by_source: &[&lang_frontend::parser::ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    types: &mut type_lower::UnitTypeLowering,
    instance: UnitPlannedInstance,
) -> Result<FunctionPlan, LoweringError> {
    let owner = instance
        .key()
        .deinit_owner()
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let descriptor = typed
        .signatures()
        .declaration(owner)
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.deinit())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let parsed = parsed_by_source
        .get(instance.source_unit().index())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let (function_item, item, _) = unwrap_modified(parsed, instance.item())?;
    let Item::Deinit { body, .. } = item else {
        return Err(lowering_error(
            LoweringErrorKind::MissingFact,
            instance.span(),
        ));
    };
    if descriptor.owner() != owner
        || descriptor.item() != UnitItemId::new(instance.source_unit(), function_item)
        || descriptor.body() != UnitStatementId::new(instance.source_unit(), body)
        || descriptor.receiver_mode() != ParameterMode::Borrow
    {
        return Err(lowering_error(
            LoweringErrorKind::MissingFact,
            instance.span(),
        ));
    }
    let receiver_type = descriptor.receiver_type();
    let owner_type = types.intern(module, typed, receiver_type, instance.span())?;
    let entity_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: owner_type,
    };
    let origin = Origin::Source(instance.span());
    let id = module
        .add_instance_function(
            instance_function_name(names, &instance)?,
            entity_type,
            Vec::new(),
            origin.clone(),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
    module
        .function_mut(id)
        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?
        .add_block(vec![entity_type], origin)
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
    module
        .set_deinit(owner_type, id)
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
    let return_type = typed
        .types()
        .builtin(BuiltinType::Unit)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let receiver = Some(ReceiverPlan {
        owner,
        mode: descriptor.receiver_mode(),
        template_ty: receiver_type,
        ty: receiver_type,
        entity_type,
        origin: instance.span(),
    });
    Ok(FunctionPlan {
        id,
        instance,
        function_item,
        body: FunctionPlanBody::Block(body),
        receiver,
        parameter_symbols: Vec::new(),
        inout_parameter: None,
        return_type,
    })
}
