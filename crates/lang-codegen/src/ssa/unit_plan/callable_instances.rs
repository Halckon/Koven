//! Unit source identities and frozen routes consume the sealed frontend provenance tables.

use super::*;
use crate::ssa::lowering_support::callable_instances::{CallableArena, CallableKey};
use lang_frontend::{
    ownership_checking::{UnitCallableOrigin, UnitPointerCallableReturnOrigin},
    parser::Expression,
    type_checking::{ParameterMode, UnitCallDescriptor},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlannedUnitCallSite {
    key: UnitFunctionInstanceKey,
    delegation: Vec<UnitDelegatedCallRoute>,
}

impl PlannedUnitCallSite {
    pub(crate) fn key(&self) -> &UnitFunctionInstanceKey {
        &self.key
    }

    pub(crate) fn delegation(&self) -> &[UnitDelegatedCallRoute] {
        &self.delegation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnitPlannedCallableReturn {
    callable: CallableToken,
    function_type: UnitTypeId,
}

impl UnitPlannedCallableReturn {
    pub(in crate::ssa) const fn callable(self) -> CallableToken {
        self.callable
    }

    pub(crate) const fn function_type(self) -> UnitTypeId {
        self.function_type
    }
}

/// Moved intact into the driver; no runtime owners or LLVM IDs occur in this plan.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct UnitCallablePlan {
    arena: CallableArena<UnitFunctionInstanceKey, UnitExpressionId>,
    routes: BTreeMap<(SourceToken, UnitExpressionId), PlannedUnitCallSite>,
    returns: BTreeMap<SourceToken, UnitPlannedCallableReturn>,
    runtime_initializers: BTreeMap<(SourceToken, UnitExpressionId), CallableToken>,
}

impl UnitCallablePlan {
    pub(in crate::ssa) fn source(&self, token: SourceToken) -> Option<&UnitFunctionInstanceKey> {
        self.arena.source(token)
    }

    pub(in crate::ssa) fn callable(
        &self,
        token: CallableToken,
    ) -> Option<&CallableKey<UnitExpressionId>> {
        self.arena.callable(token)
    }

    pub(in crate::ssa) fn call_site(
        &self,
        caller: SourceToken,
        expression: UnitExpressionId,
    ) -> Option<&PlannedUnitCallSite> {
        self.routes.get(&(caller, expression))
    }

    pub(in crate::ssa) fn runtime_initializer(
        &self,
        source: SourceToken,
        constructor: UnitExpressionId,
    ) -> Option<CallableToken> {
        self.runtime_initializers
            .get(&(source, constructor))
            .copied()
    }

    pub(in crate::ssa) fn callable_return(
        &self,
        factory: SourceToken,
    ) -> Option<UnitPlannedCallableReturn> {
        self.returns.get(&factory).copied()
    }
}

pub(super) struct CallablePlanner<'a> {
    plan: UnitCallablePlan,
    parsed: &'a [&'a ParsedFile],
    typed: &'a CompilationUnitTypes,
    owned: &'a CompilationUnitOwnership,
    templates: &'a [UnitFunctionTemplate],
    by_target: &'a BTreeMap<UnitCallableTarget, usize>,
}

impl<'a> CallablePlanner<'a> {
    pub(super) fn new(
        parsed: &'a [&'a ParsedFile],
        typed: &'a CompilationUnitTypes,
        owned: &'a CompilationUnitOwnership,
        templates: &'a [UnitFunctionTemplate],
        by_target: &'a BTreeMap<UnitCallableTarget, usize>,
    ) -> Self {
        Self {
            plan: UnitCallablePlan::default(),
            parsed,
            typed,
            owned,
            templates,
            by_target,
        }
    }

    pub(super) fn reserve_source(
        &mut self,
        key: &UnitFunctionInstanceKey,
        span: Span,
    ) -> Result<SourceToken, LoweringError> {
        self.plan
            .arena
            .intern_source(key.clone())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    pub(super) fn freeze_runtime_initializers(
        &mut self,
        caller: SourceToken,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let key = self
            .plan
            .source(caller)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if key.deinit_owner().is_some() {
            return Ok(());
        }
        let template = self
            .by_target
            .get(&key.target())
            .and_then(|index| self.templates.get(*index))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let source_unit = template.source_unit;
        let source_span = template.span;
        let parsed = self
            .parsed
            .get(source_unit.index())
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        for descriptor in self.typed.container_constructions() {
            if descriptor.expression().source_unit() != source_unit
                || descriptor.kind()
                    != lang_frontend::type_checking::ContainerConstructionKind::RuntimeLength
            {
                continue;
            }
            let node = parsed
                .ast()
                .expressions()
                .get(descriptor.expression().expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !span_contains(source_span, node.span()) {
                continue;
            }
            let Expression::Call { arguments, .. } = node.payload() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
            };
            let [_, initializer] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
            };
            if descriptor.parameter_modes() != [ParameterMode::Borrow, ParameterMode::Borrow] {
                return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
            }
            let initializer_id = UnitExpressionId::new(source_unit, initializer.value);
            let operand_span = parsed
                .ast()
                .expressions()
                .get(initializer.value)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, initializer.span))?
                .span();
            let token =
                self.origin(caller, substitutions, initializer_id, pending, operand_span)?;
            self.plan
                .runtime_initializers
                .insert((caller, descriptor.expression()), token);
        }
        Ok(())
    }

    /// Resource bodies are enabled only for lambdas selected by the frozen callable routes.
    pub(super) fn selected_resource_lambdas(
        &self,
        source: SourceToken,
    ) -> BTreeSet<UnitExpressionId> {
        let tokens = self
            .plan
            .routes
            .iter()
            .filter(|((caller, _), _)| *caller == source)
            .flat_map(|(_, route)| {
                route
                    .key
                    .callable_arguments()
                    .iter()
                    .map(|(_, token)| *token)
            })
            .chain(
                self.plan
                    .runtime_initializers
                    .iter()
                    .filter(|((caller, _), _)| *caller == source)
                    .map(|(_, token)| *token),
            )
            .chain(
                self.plan
                    .callable_return(source)
                    .map(|value| value.callable()),
            );
        tokens
            .filter_map(|token| match self.plan.callable(token) {
                Some(CallableKey::Lambda { owner, expression }) if *owner == source => {
                    Some(*expression)
                }
                _ => None,
            })
            .collect()
    }

    fn substitutions(
        &self,
        key: &UnitFunctionInstanceKey,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, UnitTypeId>, LoweringError> {
        let template = self
            .by_target
            .get(&key.target())
            .and_then(|index| self.templates.get(*index))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if template.type_parameters.len() != key.type_arguments().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(template
            .type_parameters
            .iter()
            .copied()
            .zip(key.type_arguments().iter().copied())
            .collect())
    }

    /// Ordinary data parameters stay on the old route; only a concrete Function needs a slot.
    fn function_type(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        static_self: Option<UnitTypeId>,
        span: Span,
    ) -> Result<Option<UnitTypeId>, LoweringError> {
        if !matches!(
            self.typed.types().get(ty),
            Some(UnitTypeKind::Function { .. } | UnitTypeKind::TypeParameter(_))
        ) {
            return Ok(None);
        }
        let concrete = resolve_concrete_type(self.typed, ty, substitutions, static_self, span)?;
        Ok(matches!(
            self.typed.types().get(concrete),
            Some(UnitTypeKind::Function { .. })
        )
        .then_some(concrete))
    }

    /// Exact declaration slots are mandatory; signature shape cannot supply an entry environment.
    pub(super) fn validate_arguments(
        &self,
        key: &UnitFunctionInstanceKey,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if key.deinit_owner().is_some() {
            return if key.callable_arguments().is_empty() {
                Ok(())
            } else {
                Err(lowering_error(LoweringErrorKind::MissingFact, span))
            };
        }
        let callable = unit_callable_signature(self.typed, key.target())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut expected = Vec::new();
        for (index, parameter) in callable.parameters().iter().enumerate() {
            if self
                .function_type(
                    parameter.ty(),
                    substitutions,
                    key.static_self(),
                    parameter.span(),
                )?
                .is_some()
            {
                if parameter.mode() != ParameterMode::Borrow {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        parameter.span(),
                    ));
                }
                expected.push(index);
            }
        }
        if expected
            != key
                .callable_arguments()
                .iter()
                .map(|(slot, _)| *slot)
                .collect::<Vec<_>>()
        {
            return Err(lowering_error(
                if key.callable_arguments().is_empty() {
                    LoweringErrorKind::UnsupportedNode
                } else {
                    LoweringErrorKind::MissingFact
                },
                span,
            ));
        }
        for &(_, token) in key.callable_arguments() {
            self.plan
                .callable(token)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        }
        Ok(())
    }

    pub(super) fn specialize_call(
        &mut self,
        caller: SourceToken,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        call: &UnitCallDescriptor,
        resolved: &mut ResolvedUnitCallInstance,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let current = self
            .plan
            .source(caller)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let parsed = self
            .parsed
            .get(call.expression().source_unit().index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = parsed
            .ast()
            .expressions()
            .get(call.expression().expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let signature = unit_callable_signature(self.typed, resolved.key().target())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let target_substitutions = self.substitutions(resolved.key(), span)?;
        let mut slots = Vec::new();
        let mut indices = BTreeSet::new();
        for mapping in call.arguments() {
            let argument = arguments
                .get(mapping.argument_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let parameter = signature
                .parameters()
                .get(mapping.parameter_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let operand_span = parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, argument.span))?
                .span();
            let Some(expected) = self.function_type(
                parameter.ty(),
                &target_substitutions,
                resolved.key().static_self(),
                operand_span,
            )?
            else {
                continue;
            };
            if !indices.insert(mapping.parameter_index()) || mapping.mode() != parameter.mode() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, operand_span));
            }
            if parameter.mode() != ParameterMode::Borrow {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    operand_span,
                ));
            }
            let mapped = resolve_concrete_type(
                self.typed,
                mapping.parameter_type(),
                substitutions,
                current.static_self(),
                operand_span,
            )?;
            let expression = UnitExpressionId::new(call.expression().source_unit(), argument.value);
            let actual = self
                .typed
                .expression_type(expression)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, operand_span))?;
            let actual = resolve_concrete_type(
                self.typed,
                actual,
                substitutions,
                current.static_self(),
                operand_span,
            )?;
            if actual != expected || mapped != expected {
                return Err(lowering_error(LoweringErrorKind::MissingFact, operand_span));
            }
            slots.push((
                mapping.parameter_index(),
                self.origin(caller, substitutions, expression, pending, operand_span)?,
            ));
        }
        slots.sort_by_key(|(slot, _)| *slot);
        resolved.key.callable_arguments = slots;
        self.validate_arguments(resolved.key(), &target_substitutions, span)?;
        self.reserve_source(resolved.key(), span)?;
        let route = PlannedUnitCallSite {
            key: resolved.key.clone(),
            delegation: resolved.delegation.clone(),
        };
        if let Some(previous) = self
            .plan
            .routes
            .insert((caller, call.expression()), route.clone())
            && previous != route
        {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }

    fn origin(
        &mut self,
        caller: SourceToken,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        expression: UnitExpressionId,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        span: Span,
    ) -> Result<CallableToken, LoweringError> {
        let fact = self
            .owned
            .callable_origin(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        match fact.origin() {
            UnitCallableOrigin::Lambda(lambda) => {
                self.owned
                    .closure(lambda)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let static_self = self
                    .plan
                    .source(caller)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
                    .static_self();
                // A selected callback origin does not supply an ABI for captured Functions.
                for capture in self.owned.captures_of(lambda) {
                    let ty = resolve_concrete_type(
                        self.typed,
                        capture.ty(),
                        substitutions,
                        static_self,
                        capture.reference_span(),
                    )?;
                    if contains_function(self.typed, ty) {
                        return Err(lowering_error(
                            LoweringErrorKind::UnsupportedNode,
                            capture.reference_span(),
                        ));
                    }
                }
                self.plan
                    .arena
                    .intern_callable(CallableKey::Lambda {
                        owner: caller,
                        expression: lambda,
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
            }
            UnitCallableOrigin::Parameter(symbol) => {
                let key = self
                    .plan
                    .source(caller)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let signature = unit_callable_signature(self.typed, key.target())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let slot = signature
                    .parameters()
                    .iter()
                    .position(|parameter| parameter.symbol() == Some(symbol))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
                key.callable_arguments()
                    .binary_search_by_key(&slot, |(slot, _)| *slot)
                    .map(|index| key.callable_arguments()[index].1)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))
            }
            UnitCallableOrigin::KnownFunction(target) => self.known_function(target, pending, span),
            UnitCallableOrigin::FactoryResult(call) => {
                self.factory_return(caller, substitutions, call, pending, span)
            }
        }
    }

    fn known_function(
        &mut self,
        target: UnitCallableTarget,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        span: Span,
    ) -> Result<CallableToken, LoweringError> {
        let signature = unit_callable_signature(self.typed, target)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(target, UnitCallableTarget::Declaration(_))
            || !signature.has_body()
            || signature.receiver().is_some()
            || !signature.type_parameters().is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let key = UnitFunctionInstanceKey::for_target(target, Vec::new());
        self.validate_arguments(&key, &BTreeMap::new(), span)?;
        let source = self.reserve_source(&key, span)?;
        pending.insert(key);
        self.plan
            .arena
            .intern_callable(CallableKey::KnownFunction { function: source })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    /// Pointer returns are memoized by the factory's complete source instance, not its call use.
    fn factory_return(
        &mut self,
        caller: SourceToken,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        expression: UnitExpressionId,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        span: Span,
    ) -> Result<CallableToken, LoweringError> {
        let current = self
            .plan
            .source(caller)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let call = self
            .typed
            .call(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitCallTarget::Declaration(declaration) = call.target() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if call.receiver().is_some() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let key = if let Some(route) = self.plan.call_site(caller, expression) {
            route.key().clone()
        } else {
            let arguments = call
                .instance()
                .type_arguments()
                .iter()
                .map(|ty| {
                    resolve_concrete_type(
                        self.typed,
                        *ty,
                        substitutions,
                        current.static_self(),
                        span,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut resolved = resolve_unit_call_instance(
                self.typed,
                self.owned,
                UnitCallableTarget::Declaration(declaration),
                arguments,
                None,
                span,
            )?;
            self.specialize_call(caller, substitutions, call, &mut resolved, pending, span)?;
            resolved.key
        };
        let factory = self.reserve_source(&key, span)?;
        pending.insert(key);
        self.freeze_pointer_return(factory, pending, span)?;
        let result = self
            .plan
            .callable_return(factory)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let actual = resolve_concrete_type(
            self.typed,
            call.return_type(),
            substitutions,
            current.static_self(),
            span,
        )?;
        if result.function_type() != actual {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(result.callable())
    }

    /// Every reachable pointer factory needs a frozen return ABI, including direct constructors.
    pub(super) fn freeze_pointer_return(
        &mut self,
        factory: SourceToken,
        pending: &mut BTreeSet<UnitFunctionInstanceKey>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.plan.callable_return(factory).is_some() {
            return Ok(());
        }
        let key = self
            .plan
            .source(factory)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(summary) = self.owned.pointer_callable_return(key.target()) else {
            return Ok(());
        };
        let signature = unit_callable_signature(self.typed, key.target())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if summary.function_type() != signature.return_type() {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                summary.span(),
            ));
        }
        let factory_substitutions = self.substitutions(&key, span)?;
        let function_type = resolve_concrete_type(
            self.typed,
            summary.function_type(),
            &factory_substitutions,
            key.static_self(),
            summary.span(),
        )?;
        let return_value_type = self
            .typed
            .expression_type(summary.return_value())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, summary.span()))?;
        let actual = resolve_concrete_type(
            self.typed,
            return_value_type,
            &factory_substitutions,
            key.static_self(),
            summary.span(),
        )?;
        if function_type != actual
            || !matches!(
                self.typed.types().get(function_type),
                Some(UnitTypeKind::Function { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let callable = match summary.origin() {
            UnitPointerCallableReturnOrigin::Lambda(lambda) => {
                let template = &self.templates[*self
                    .by_target
                    .get(&key.target())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?];
                let node = self
                    .parsed
                    .get(lambda.source_unit().index())
                    .and_then(|parsed| parsed.ast().expressions().get(lambda.expression()).ok())
                    .ok_or_else(|| {
                        lowering_error(LoweringErrorKind::MissingFact, summary.span())
                    })?;
                if lambda.source_unit() != template.source_unit
                    || !span_contains(template.span, node.span())
                    || self.owned.closure(lambda).is_none()
                    || self.owned.captures_of(lambda).next().is_some()
                {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        summary.span(),
                    ));
                }
                self.plan
                    .arena
                    .intern_callable(CallableKey::Lambda {
                        owner: factory,
                        expression: lambda,
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, summary.span()))?
            }
            UnitPointerCallableReturnOrigin::KnownFunction(target) => {
                self.known_function(target, pending, summary.span())?
            }
        };
        self.plan.returns.insert(
            factory,
            UnitPlannedCallableReturn {
                callable,
                function_type,
            },
        );
        Ok(())
    }

    pub(super) fn finish(
        self,
        instances: &[UnitPlannedInstance],
    ) -> Result<UnitCallablePlan, LoweringError> {
        for instance in instances {
            if self.plan.source(instance.source_token()) != Some(instance.key()) {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    instance.span(),
                ));
            }
        }
        for result in self.plan.returns.values() {
            if self.plan.callable(result.callable()).is_none()
                || !matches!(
                    self.typed.types().get(result.function_type()),
                    Some(UnitTypeKind::Function { .. })
                )
            {
                return Err(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                });
            }
        }
        Ok(self.plan)
    }
}

fn contains_function(typed: &CompilationUnitTypes, ty: UnitTypeId) -> bool {
    match typed.types().get(ty) {
        Some(UnitTypeKind::Function { .. }) => true,
        Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner)) => {
            contains_function(typed, *inner)
        }
        Some(
            UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. },
        ) => arguments
            .iter()
            .any(|argument| contains_function(typed, *argument)),
        _ => false,
    }
}
