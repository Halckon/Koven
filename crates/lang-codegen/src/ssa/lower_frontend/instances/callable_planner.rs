//! Deterministic source worklist; callback identity lives in the shared flat arena.

use super::*;
use lang_frontend::type_checking::{CallableDescriptor, CallableTarget};
use std::collections::BTreeSet;

mod origins;

pub(super) struct Planner<'a> {
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

pub(super) fn plan(
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
        .filter(|signature| signature.owner().is_none())
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
        if template.type_parameters.is_empty()
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
    fn signature(
        &self,
        symbol: SymbolId,
        span: Span,
    ) -> Result<&'a CallableDescriptor, LoweringError> {
        self.signatures
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn expression_span(&self, expression: ExpressionId) -> Result<Span, LoweringError> {
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

    fn enqueue(
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

    fn substitutions(
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

    fn resolve_call(
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
}

fn index_calls(
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
