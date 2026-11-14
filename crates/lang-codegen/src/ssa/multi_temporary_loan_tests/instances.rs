//! 从 SPEC-0177 callable key 建立确定的具体泛型实例图。

use std::{cmp::Ordering, collections::BTreeMap};

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::OwnershipCheckedFile,
    parser::ParsedFile,
    source::Span,
    type_checking::{FunctionParameterType, IntrinsicTypeConstructor, TypeId, TypeKind, TypedFile},
};

use crate::ssa::lower_frontend::{LoweringError, LoweringErrorKind, error};
use crate::ssa::lowering_support::callable_instances::{
    CallableArena, CallableKey, CallableToken, SourceIdentity, SourceToken,
};

/// 防止合法但病态的源码让单次标量 lowering 无界扩张。
pub(in crate::ssa) const MAX_GENERIC_INSTANCES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ssa) struct FunctionInstanceKey {
    symbol: SymbolId,
    type_arguments: Vec<TypeId>,
    callable_arguments: Vec<(usize, CallableToken)>,
}

impl FunctionInstanceKey {
    pub(in crate::ssa) fn new(symbol: SymbolId, type_arguments: Vec<TypeId>) -> Self {
        Self {
            symbol,
            type_arguments,
            callable_arguments: Vec::new(),
        }
    }

    pub(in crate::ssa) const fn symbol(&self) -> SymbolId {
        self.symbol
    }

    pub(in crate::ssa) fn type_arguments(&self) -> &[TypeId] {
        &self.type_arguments
    }

    pub(in crate::ssa) fn callable_arguments(&self) -> &[(usize, CallableToken)] {
        &self.callable_arguments
    }

    pub(in crate::ssa) fn is_specialized(&self) -> bool {
        !self.type_arguments.is_empty() || !self.callable_arguments.is_empty()
    }
}

impl SourceIdentity for FunctionInstanceKey {
    fn callable_arguments(&self) -> &[(usize, CallableToken)] {
        self.callable_arguments()
    }
}

/// AST IDs compare by index within this planner's already validated single-file chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) struct OrderedExpressionId(ExpressionId);

impl OrderedExpressionId {
    pub(in crate::ssa) const fn expression(self) -> ExpressionId {
        self.0
    }
}

impl Ord for OrderedExpressionId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.index().cmp(&other.0.index())
    }
}

impl PartialOrd for OrderedExpressionId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(in crate::ssa) struct FunctionTemplate {
    pub(in crate::ssa) symbol: SymbolId,
    pub(in crate::ssa) type_parameters: Vec<SymbolId>,
    pub(in crate::ssa) span: Span,
}

pub(in crate::ssa) struct PlannedInstance {
    pub(in crate::ssa) source: SourceToken,
    pub(in crate::ssa) key: FunctionInstanceKey,
    pub(in crate::ssa) template_index: usize,
    pub(in crate::ssa) substitutions: BTreeMap<SymbolId, TypeId>,
}

/// Frozen source routes retain complete callback slots independently of unfinished closure ABI.
#[derive(Default)]
pub(in crate::ssa) struct FunctionInstancePlan {
    instances: Vec<PlannedInstance>,
    arena: CallableArena<FunctionInstanceKey, OrderedExpressionId>,
    routes: BTreeMap<(SourceToken, OrderedExpressionId), FunctionInstanceKey>,
    pointer_returns: BTreeMap<SourceToken, Option<CallableToken>>,
}

impl FunctionInstancePlan {
    pub(in crate::ssa) fn instances(&self) -> &[PlannedInstance] {
        &self.instances
    }

    pub(in crate::ssa) fn source(&self, source: SourceToken) -> Option<&FunctionInstanceKey> {
        self.arena.source(source)
    }

    pub(in crate::ssa) fn callable(
        &self,
        callable: CallableToken,
    ) -> Option<&CallableKey<OrderedExpressionId>> {
        self.arena.callable(callable)
    }

    pub(in crate::ssa) fn call_site(
        &self,
        caller: SourceToken,
        expression: ExpressionId,
    ) -> Option<&FunctionInstanceKey> {
        self.routes.get(&(caller, OrderedExpressionId(expression)))
    }

    pub(in crate::ssa) fn pointer_return(&self, source: SourceToken) -> Option<CallableToken> {
        self.pointer_returns.get(&source).copied().flatten()
    }
}

/// The private caller must validate Names -> Typed -> Owned analysis identity first.
/// Orchestrate's validate_inputs is authoritative; matching source IDs alone is not a witness.
pub(in crate::ssa) fn plan_instances(
    parsed: &ParsedFile,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    templates: &[FunctionTemplate],
    deinit_spans: &[Span],
) -> Result<FunctionInstancePlan, LoweringError> {
    callable_planner::plan(parsed, typed, owned, templates, deinit_spans)
}

pub(in crate::ssa) fn resolve_concrete_type(
    typed: &TypedFile,
    ty: TypeId,
    substitutions: &BTreeMap<SymbolId, TypeId>,
    span: Span,
) -> Result<TypeId, LoweringError> {
    match typed.types().get(ty) {
        Some(TypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
        Some(TypeKind::Function {
            move_only,
            parameters,
            return_type,
        }) => {
            let parameters = parameters
                .iter()
                .map(|parameter| {
                    Ok(FunctionParameterType {
                        mode: parameter.mode,
                        ty: resolve_concrete_type(typed, parameter.ty, substitutions, span)?,
                    })
                })
                .collect::<Result<Vec<_>, LoweringError>>()?;
            let return_type = resolve_concrete_type(typed, *return_type, substitutions, span)?;
            typed
                .types()
                .find(&TypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }
        Some(TypeKind::Intrinsic {
            constructor,
            arguments,
        }) if matches!(
            constructor,
            IntrinsicTypeConstructor::View
                | IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList
        ) =>
        {
            let arguments = arguments
                .iter()
                .map(|argument| resolve_concrete_type(typed, *argument, substitutions, span))
                .collect::<Result<Vec<_>, _>>()?;
            typed
                .types()
                .find(&TypeKind::Intrinsic {
                    constructor: *constructor,
                    arguments,
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(error(LoweringErrorKind::MissingFact, span)),
    }
}

pub(in crate::ssa) fn contains_type_parameter(typed: &TypedFile, kind: &TypeKind) -> bool {
    let contains = |ty| {
        typed
            .types()
            .get(ty)
            .is_some_and(|kind| contains_type_parameter(typed, kind))
    };
    match kind {
        TypeKind::TypeParameter(_) => true,
        TypeKind::Nullable(inner) | TypeKind::StaticSelf(inner) => contains(*inner),
        TypeKind::Function {
            parameters,
            return_type,
            ..
        } => parameters.iter().any(|parameter| contains(parameter.ty)) || contains(*return_type),
        TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. } => {
            arguments.iter().copied().any(contains)
        }
        TypeKind::EnumCase { root, .. } => contains(*root),
        TypeKind::Builtin(_)
        | TypeKind::Capability(_)
        | TypeKind::IntegerLiteral(_)
        | TypeKind::Error
        | TypeKind::Deferred(_) => false,
    }
}

pub(in crate::ssa) fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

pub(in crate::ssa) fn generic_instance_budget_exhausted(count: usize) -> bool {
    count >= MAX_GENERIC_INSTANCES
}

#[cfg(test)]
mod tests {
    use crate::ssa::lower_frontend::instances::{
        MAX_GENERIC_INSTANCES, generic_instance_budget_exhausted,
    };

    #[test]
    pub(in crate::ssa) fn generic_instance_budget_has_an_explicit_boundary() {
        assert!(!generic_instance_budget_exhausted(
            MAX_GENERIC_INSTANCES - 1
        ));
        assert!(generic_instance_budget_exhausted(MAX_GENERIC_INSTANCES));
    }
}

#[cfg(test)]
pub(in crate::ssa) use crate::ssa::unit_llvm_tests::callable_plan_tests;

mod callable_planner {
    //! Deterministic source worklist; callback identity lives in the shared flat arena.
    use super::*;

    use crate::ssa::lower_frontend::*;
    use lang_frontend::{
        ownership_checking::{CallableOrigin, PointerCallableReturnOrigin},
        parser::Expression,
        type_checking::{CallDescriptor, CallableDescriptor, CallableTarget, ParameterMode},
    };
    use std::collections::BTreeSet;

    pub(in crate::ssa) struct Planner<'a> {
        parsed: &'a ParsedFile,
        typed: &'a TypedFile,
        owned: &'a OwnershipCheckedFile,
        templates: &'a [FunctionTemplate],
        template_by_symbol: BTreeMap<SymbolId, usize>,
        signatures: BTreeMap<SymbolId, &'a CallableDescriptor>,
        calls_by_template: Vec<Vec<(usize, Span)>>,
        call_by_expression: BTreeMap<usize, usize>,
        plan: FunctionInstancePlan,
        pending: BTreeSet<FunctionInstanceKey>,
        resolving_calls: BTreeSet<(Option<SourceToken>, OrderedExpressionId)>,
    }

    pub(in crate::ssa) fn plan(
        parsed: &ParsedFile,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        templates: &[FunctionTemplate],
        deinit_spans: &[Span],
    ) -> Result<FunctionInstancePlan, LoweringError> {
        if parsed.source_id() != typed.source_id() || parsed.source_id() != owned.source_id() {
            return Err(LoweringError {
                kind: LoweringErrorKind::MismatchedSource,
                span: None,
            });
        }
        if !parsed.diagnostics().is_empty()
            || !typed.diagnostics().is_empty()
            || !owned.diagnostics().is_empty()
        {
            return Err(LoweringError {
                kind: LoweringErrorKind::FrontendDiagnostics,
                span: None,
            });
        }
        if let Some(window) = templates.windows(2).find(|window| {
            window[0].span.source_id() != window[1].span.source_id()
                || window[0].span.start() > window[1].span.start()
        }) {
            return Err(error(LoweringErrorKind::MissingFact, window[1].span));
        }
        let signatures = typed
            .callables()
            .iter()
            .filter(|signature| {
                templates
                    .iter()
                    .any(|template| template.symbol == signature.symbol())
            })
            .map(|signature| (signature.symbol(), signature))
            .collect();
        let mut planner = Planner {
            parsed,
            typed,
            owned,
            templates,
            template_by_symbol: templates
                .iter()
                .enumerate()
                .map(|(index, template)| (template.symbol, index))
                .collect(),
            signatures,
            calls_by_template: index_calls(parsed, typed, templates)?,
            call_by_expression: typed
                .calls()
                .iter()
                .enumerate()
                .map(|(index, call)| (call.expression().index(), index))
                .collect(),
            plan: FunctionInstancePlan::default(),
            pending: BTreeSet::new(),
            resolving_calls: BTreeSet::new(),
        };
        for template in templates {
            let signature = planner.signature(template.symbol, template.span)?;
            // A Fn parameter has no concrete environment until an actual caller supplies its origin.
            if signature.owner().is_none()
                && template.type_parameters.is_empty()
                && !signature.parameters().iter().any(|parameter| {
                    matches!(
                        typed.types().get(parameter.ty),
                        Some(TypeKind::Function { .. })
                    )
                })
            {
                planner.enqueue(
                    FunctionInstanceKey::new(template.symbol, Vec::new()),
                    template.span,
                )?;
            }
        }
        // Preserve scalar destructor call roots without inventing a source owner for deinit lambdas.
        for (index, call) in typed.calls().iter().enumerate() {
            let span = planner.expression_span(call.expression())?;
            if deinit_spans.iter().any(|owner| span_contains(*owner, span)) {
                planner.resolve_call(None, index, &BTreeMap::new())?;
            }
        }
        let mut planned = BTreeMap::new();
        let mut specialized_count = 0;
        while let Some(key) = planner.pending.pop_first() {
            if planned.contains_key(&key) {
                continue;
            }
            let template_index =
                *planner
                    .template_by_symbol
                    .get(&key.symbol())
                    .ok_or(LoweringError {
                        kind: LoweringErrorKind::MissingFact,
                        span: None,
                    })?;
            let template = &templates[template_index];
            if template.type_parameters.len() != key.type_arguments().len() {
                return Err(error(LoweringErrorKind::MissingFact, template.span));
            }
            // Dedup first; a source with both generic and callback specialization is charged once.
            if key.is_specialized() && generic_instance_budget_exhausted(specialized_count) {
                return Err(error(
                    LoweringErrorKind::InstanceLimitExceeded,
                    template.span,
                ));
            }
            let source = planner
                .plan
                .arena
                .intern_source(key.clone())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, template.span))?;
            let substitutions = template
                .type_parameters
                .iter()
                .copied()
                .zip(key.type_arguments().iter().copied())
                .collect::<BTreeMap<_, _>>();
            if key.is_specialized() {
                specialized_count += 1;
            }
            planner.freeze_pointer_return(source, template.span)?;
            // Move this template's index out during mutable resolution, then reuse it for other owners.
            let calls = std::mem::take(&mut planner.calls_by_template[template_index]);
            for (call_index, _) in &calls {
                planner.resolve_call(Some(source), *call_index, &substitutions)?;
            }
            planner.calls_by_template[template_index] = calls;
            planned.insert(
                key.clone(),
                PlannedInstance {
                    source,
                    key,
                    template_index,
                    substitutions,
                },
            );
        }
        planner.plan.instances = planned.into_values().collect();
        Ok(planner.plan)
    }

    impl<'a> Planner<'a> {
        pub(in crate::ssa) fn signature(
            &self,
            symbol: SymbolId,
            span: Span,
        ) -> Result<&'a CallableDescriptor, LoweringError> {
            self.signatures
                .get(&symbol)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }

        pub(in crate::ssa) fn expression_span(
            &self,
            expression: ExpressionId,
        ) -> Result<Span, LoweringError> {
            self.parsed
                .ast()
                .expressions()
                .get(expression)
                .map(|node| node.span())
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })
        }

        pub(in crate::ssa) fn enqueue(
            &mut self,
            key: FunctionInstanceKey,
            span: Span,
        ) -> Result<SourceToken, LoweringError> {
            let token = self
                .plan
                .arena
                .intern_source(key.clone())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.pending.insert(key);
            Ok(token)
        }

        pub(in crate::ssa) fn substitutions(
            &self,
            source: SourceToken,
            span: Span,
        ) -> Result<BTreeMap<SymbolId, TypeId>, LoweringError> {
            let key = self
                .plan
                .source(source)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let signature = self.signature(key.symbol(), span)?;
            if signature.type_parameters().len() != key.type_arguments().len() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            Ok(signature
                .type_parameters()
                .iter()
                .copied()
                .zip(key.type_arguments().iter().copied())
                .collect())
        }

        pub(in crate::ssa) fn resolve_call(
            &mut self,
            caller: Option<SourceToken>,
            index: usize,
            substitutions: &BTreeMap<SymbolId, TypeId>,
        ) -> Result<Option<FunctionInstanceKey>, LoweringError> {
            let typed = self.typed;
            let call = typed.calls().get(index).ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
            let expression = call.expression();
            if let Some(key) = caller.and_then(|caller| self.plan.call_site(caller, expression)) {
                return Ok(Some(key.clone()));
            }
            let CallableTarget::Source(target) = call.target() else {
                return Ok(None);
            };
            let Some(&template_index) = self.template_by_symbol.get(&target) else {
                return Ok(None);
            };
            let span = self.expression_span(expression)?;
            let guard = (caller, OrderedExpressionId(expression));
            if !self.resolving_calls.insert(guard) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let result = (|| {
                let arguments = call
                    .instance()
                    .type_arguments()
                    .iter()
                    .map(|ty| resolve_concrete_type(typed, *ty, substitutions, span))
                    .collect::<Result<Vec<_>, _>>()?;
                if arguments.len() != self.templates[template_index].type_parameters.len() {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let mut key = FunctionInstanceKey::new(target, arguments);
                key.callable_arguments =
                    self.callback_slots(caller, call, &key, substitutions, span)?;
                self.enqueue(key.clone(), span)?;
                if let Some(caller) = caller {
                    self.plan
                        .routes
                        .insert((caller, OrderedExpressionId(expression)), key.clone());
                }
                Ok(Some(key))
            })();
            self.resolving_calls.remove(&guard);
            result
        }

        pub(in crate::ssa) fn callback_slots(
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
                let token =
                    self.origin(caller, fact.origin(), caller_substitutions, argument_span)?;
                slots.push((parameter_index, token));
            }
            Ok(slots)
        }

        pub(in crate::ssa) fn origin(
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

        pub(in crate::ssa) fn lambda(
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

        pub(in crate::ssa) fn known_function(
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
        pub(in crate::ssa) fn freeze_pointer_return(
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

    pub(in crate::ssa) fn contains_function(typed: &TypedFile, ty: TypeId) -> bool {
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

    pub(in crate::ssa) fn index_calls(
        parsed: &ParsedFile,
        typed: &TypedFile,
        templates: &[FunctionTemplate],
    ) -> Result<Vec<Vec<(usize, Span)>>, LoweringError> {
        let mut calls = vec![Vec::new(); templates.len()];
        for (call_index, call) in typed.calls().iter().enumerate() {
            let span = parsed
                .ast()
                .expressions()
                .get(call.expression())
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?
                .span();
            let upper = templates.partition_point(|template| template.span.start() <= span.start());
            if let Some(index) = upper
                .checked_sub(1)
                .filter(|index| span_contains(templates[*index].span, span))
            {
                calls[index].push((call_index, span));
            }
        }
        Ok(calls)
    }
}
