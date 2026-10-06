//! Resolve committed callable facts into sparse slots; never infer an environment from Fn type.

use super::*;
use lang_frontend::{
    ownership_checking::{CallableOrigin, PointerCallableReturnOrigin},
    parser::Expression,
    type_checking::{CallDescriptor, ParameterMode},
};

impl Planner<'_> {
    pub(super) fn callback_slots(
        &mut self,
        caller: Option<SourceToken>,
        call: &CallDescriptor,
        callee: &FunctionInstanceKey,
        caller_substitutions: &BTreeMap<SymbolId, TypeId>,
        span: Span,
    ) -> Result<Vec<(usize, CallableToken)>, LoweringError> {
        let signature = self.signature(callee.symbol(), span)?;
        let callee_substitutions = signature
            .type_parameters()
            .iter()
            .copied()
            .zip(callee.type_arguments().iter().copied())
            .collect::<BTreeMap<_, _>>();
        let parsed = self.parsed;
        let Expression::Call { arguments, .. } = parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if call.arguments().len() != arguments.len()
            || call.arguments().len() != signature.parameters().len()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut slots = Vec::new();
        for (parameter_index, parameter) in signature.parameters().iter().enumerate() {
            // Ordinary data wrappers remain on the original source frontier, outside Fn discovery.
            if !matches!(
                self.typed.types().get(parameter.ty),
                Some(TypeKind::Function { .. } | TypeKind::TypeParameter(_))
            ) {
                continue;
            }
            let expected =
                resolve_concrete_type(self.typed, parameter.ty, &callee_substitutions, span)?;
            if !matches!(
                self.typed.types().get(expected),
                Some(TypeKind::Function { .. })
            ) {
                continue;
            }
            let mut mappings = call
                .arguments()
                .iter()
                .filter(|mapping| mapping.parameter_index() == parameter_index);
            let mapping = mappings
                .next()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if mappings.next().is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let argument = arguments
                .get(mapping.argument_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let argument_span = self.expression_span(argument.value)?;
            if parameter.mode != ParameterMode::Borrow || mapping.mode() != parameter.mode {
                return Err(error(LoweringErrorKind::UnsupportedNode, argument_span));
            }
            let mapped = resolve_concrete_type(
                self.typed,
                mapping.parameter_type(),
                caller_substitutions,
                argument_span,
            )?;
            let actual = self
                .typed
                .expression_type(argument.value)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument_span))?;
            let actual =
                resolve_concrete_type(self.typed, actual, caller_substitutions, argument_span)?;
            if mapped != expected {
                return Err(error(LoweringErrorKind::MissingFact, argument_span));
            }
            if actual != expected
                || !matches!(
                    self.typed.types().get(actual),
                    Some(TypeKind::Function { .. })
                )
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, argument_span));
            }
            let fact = self
                .owned
                .callable_origin(argument.value)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, argument_span))?;
            if fact.expression() != argument.value || fact.span() != argument_span {
                return Err(error(LoweringErrorKind::MissingFact, argument_span));
            }
            let token = self.origin(caller, fact.origin(), caller_substitutions, argument_span)?;
            slots.push((parameter_index, token));
        }
        Ok(slots)
    }

    fn origin(
        &mut self,
        caller: Option<SourceToken>,
        origin: CallableOrigin,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        span: Span,
    ) -> Result<CallableToken, LoweringError> {
        match origin {
            CallableOrigin::Lambda(expression) => {
                let owner =
                    caller.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                self.lambda(owner, expression, substitutions, span)
            }
            CallableOrigin::Parameter(symbol) => {
                let owner =
                    caller.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let key = self
                    .plan
                    .source(owner)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let signature = self.signature(key.symbol(), span)?;
                // A lambda's own Fn parameter is not a parameter of this named source instance.
                let position = signature
                    .parameter_symbols()
                    .iter()
                    .position(|parameter| *parameter == Some(symbol))
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let parameter = &signature.parameters()[position];
                if parameter.mode != ParameterMode::Borrow {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let token = key
                    .callable_arguments()
                    .iter()
                    .find(|(slot, _)| *slot == position)
                    .map(|(_, token)| *token)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.plan
                    .callable(token)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                Ok(token)
            }
            CallableOrigin::KnownFunction(symbol) => self.known_function(symbol, span),
            CallableOrigin::FactoryResult(expression) => {
                let index = *self
                    .call_by_expression
                    .get(&expression.index())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let key = self
                    .resolve_call(caller, index, substitutions)?
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let source = self
                    .plan
                    .arena
                    .intern_source(key)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.freeze_pointer_return(source, span)?;
                let token = self
                    .plan
                    .pointer_return(source)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                // The declaration summary identity and this call's concrete return are distinct.
                let signature = self.signature(
                    self.plan
                        .source(source)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                        .symbol(),
                    span,
                )?;
                let factory_substitutions = self.substitutions(source, span)?;
                let declaration_return = resolve_concrete_type(
                    self.typed,
                    signature.return_type(),
                    &factory_substitutions,
                    span,
                )?;
                let call_return = resolve_concrete_type(
                    self.typed,
                    self.typed.calls()[index].return_type(),
                    substitutions,
                    span,
                )?;
                if declaration_return != call_return {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                Ok(token)
            }
        }
    }

    fn lambda(
        &mut self,
        owner: SourceToken,
        expression: ExpressionId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        span: Span,
    ) -> Result<CallableToken, LoweringError> {
        let ordered = OrderedExpressionId(expression);
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(ordered.expression())
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Lambda { .. })
            || self.owned.closure(expression).is_none()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let key = self
            .plan
            .source(owner)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let template = &self.templates[*self
            .template_by_symbol
            .get(&key.symbol())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?];
        if !span_contains(template.span, node.span()) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Capture facts remain authoritative. Function-valued captures have no selected layout.
        for capture in self.owned.captures_of(expression) {
            let ty = resolve_concrete_type(
                self.typed,
                capture.ty(),
                substitutions,
                capture.reference_span(),
            )?;
            if contains_function(self.typed, ty) {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    capture.reference_span(),
                ));
            }
        }
        self.plan
            .arena
            .intern_callable(CallableKey::Lambda {
                owner,
                expression: ordered,
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn known_function(
        &mut self,
        symbol: SymbolId,
        span: Span,
    ) -> Result<CallableToken, LoweringError> {
        let signature = self.signature(symbol, span)?;
        if !signature.type_parameters().is_empty()
            || !self.template_by_symbol.contains_key(&symbol)
            || signature.parameters().iter().any(|parameter| {
                matches!(
                    self.typed.types().get(parameter.ty),
                    Some(TypeKind::Function { .. })
                )
            })
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let function = self.enqueue(FunctionInstanceKey::new(symbol, Vec::new()), span)?;
        self.plan
            .arena
            .intern_callable(CallableKey::KnownFunction { function })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    /// Freeze one summary per concrete factory source, including the absence of a pointer return.
    pub(super) fn freeze_pointer_return(
        &mut self,
        source: SourceToken,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.plan.pointer_returns.contains_key(&source) {
            return Ok(());
        }
        let symbol = self
            .plan
            .source(source)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .symbol();
        let Some(summary) = self.owned.pointer_callable_return(symbol) else {
            self.plan.pointer_returns.insert(source, None);
            return Ok(());
        };
        let signature = self.signature(symbol, span)?;
        if summary.target() != symbol || summary.function_type() != signature.return_type() {
            return Err(error(LoweringErrorKind::MissingFact, summary.span()));
        }
        let substitutions = self.substitutions(source, span)?;
        let declared = resolve_concrete_type(
            self.typed,
            summary.function_type(),
            &substitutions,
            summary.span(),
        )?;
        let actual = self
            .typed
            .expression_type(summary.return_value())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, summary.span()))?;
        let actual = resolve_concrete_type(self.typed, actual, &substitutions, summary.span())?;
        if declared != actual
            || !matches!(
                self.typed.types().get(declared),
                Some(TypeKind::Function { .. })
            )
        {
            return Err(error(LoweringErrorKind::MissingFact, summary.span()));
        }
        let token = match summary.origin() {
            PointerCallableReturnOrigin::Lambda(expression) => {
                if self.owned.captures_of(expression).next().is_some() {
                    return Err(error(LoweringErrorKind::MissingFact, summary.span()));
                }
                self.lambda(source, expression, &substitutions, summary.span())?
            }
            PointerCallableReturnOrigin::KnownFunction(symbol) => {
                self.known_function(symbol, summary.span())?
            }
        };
        self.plan.pointer_returns.insert(source, Some(token));
        Ok(())
    }
}

fn contains_function(typed: &TypedFile, ty: TypeId) -> bool {
    match typed.types().get(ty) {
        Some(TypeKind::Function { .. }) => true,
        Some(TypeKind::Nullable(inner) | TypeKind::StaticSelf(inner)) => {
            contains_function(typed, *inner)
        }
        Some(TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. }) => {
            arguments
                .iter()
                .any(|argument| contains_function(typed, *argument))
        }
        _ => false,
    }
}
