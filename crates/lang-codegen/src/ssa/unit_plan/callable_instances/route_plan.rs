//! Frozen callable route storage, independent of ownership queries.
use super::*;

impl UnitCallablePlan {
    pub(super) fn reserve_source(
        &mut self,
        key: &UnitFunctionInstanceKey,
        span: Span,
    ) -> Result<SourceToken, LoweringError> {
        self.arena
            .intern_source(key.clone())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    // Actual arena reservation and route publication after the caller's
    // checked argument and dispatch decision. No ownership provider needed.
    pub(super) fn record_checked_route(
        &mut self,
        caller: SourceToken,
        expression: UnitExpressionId,
        resolved: &ResolvedUnitCallInstance,
        dispatches: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        if let Some(previous) = self.call_site(caller, expression) {
            if previous.key() != resolved.key()
                || previous.delegation() != resolved.delegation()
                || previous.dispatches != dispatches
            {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
            return Ok(());
        }

        // Do not create a caller bucket or publish an entry before the dispatch
        // source is actually registered.
        if dispatches {
            self.arena
                .intern_source(resolved.key().clone())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        }
        self.routes.entry(caller).or_default().insert(
            expression,
            PlannedUnitCallSite {
                key: resolved.key.clone(),
                delegation: resolved.delegation.clone(),
                dispatches,
            },
        );

        Ok(())
    }

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
        self.routes.get(&caller)?.get(&expression)
    }

    pub(in crate::ssa) fn runtime_callback(
        &self,
        source: SourceToken,
        constructor: UnitExpressionId,
    ) -> Option<CallableToken> {
        self.runtime_callbacks
            .get(&source)?
            .get(&constructor)
            .copied()
    }

    pub(in crate::ssa) fn routes_for(
        &self,
        source: SourceToken,
    ) -> impl ExactSizeIterator<Item = &PlannedUnitCallSite> {
        self.routes
            .get(&source)
            .map(BTreeMap::values)
            .unwrap_or_default()
    }

    pub(super) fn runtime_callbacks_for(
        &self,
        source: SourceToken,
    ) -> impl ExactSizeIterator<Item = CallableToken> + '_ {
        self.runtime_callbacks
            .get(&source)
            .map(BTreeMap::values)
            .unwrap_or_default()
            .copied()
    }

    #[cfg(test)]
    pub(in crate::ssa) fn borrowed_argument(
        &self,
        source: SourceToken,
        argument: UnitExpressionId,
    ) -> Option<CallableToken> {
        self.borrowed_arguments
            .get(&source)?
            .get(&argument)
            .copied()
    }

    /// Every concrete callable a source passes to a Borrow function parameter.
    pub(in crate::ssa) fn borrowed_arguments(
        &self,
        source: SourceToken,
    ) -> impl ExactSizeIterator<Item = CallableToken> + '_ {
        self.borrowed_arguments
            .get(&source)
            .map(BTreeMap::values)
            .unwrap_or_default()
            .copied()
    }

    pub(in crate::ssa) fn prefix_views(&self) -> impl Iterator<Item = (UnitTypeId, Span)> + '_ {
        self.prefix_views.iter().map(|(&ty, &span)| (ty, span))
    }

    pub(in crate::ssa) fn callable_return(
        &self,
        factory: SourceToken,
    ) -> Option<UnitPlannedCallableReturn> {
        self.returns.get(&factory).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_frontend::{
        lexer::lex,
        name_resolution::{SourceUnitInput, index_compilation_unit},
        parser::parse_file,
        source::SourceMap,
    };

    #[test]
    fn conflicting_call_route_keeps_original_and_does_not_reserve_dispatch() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "routes.ko",
                "fun first(): Int = 1\nfun second(): Int = 2\nfun root(): Int = first()",
            )
            .unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(file.diagnostics().is_empty());
        let inputs = [SourceUnitInput::memory_document(source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let unit = index.source_units()[0].id();
        let (expression, span) = file
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(node.payload(), Expression::Call { .. })
                    .then_some((UnitExpressionId::new(unit, id), node.span()))
            })
            .expect("original call");
        let resolved = |name| {
            let declaration = index
                .declarations()
                .iter()
                .find(|declaration| declaration.name() == name)
                .unwrap()
                .id();
            ResolvedUnitCallInstance {
                key: UnitFunctionInstanceKey {
                    target: UnitCallableTarget::Declaration(declaration),
                    type_arguments: Vec::new(),
                    static_self: None,
                    deinit: false,
                    callable_arguments: Vec::new(),
                },
                delegation: Vec::new(),
                dependent_owner_types: BTreeSet::new(),
            }
        };
        let root = resolved("root");
        let first = resolved("first");
        let second = resolved("second");
        let mut plan = UnitCallablePlan::default();
        let caller = plan.reserve_source(root.key(), span).unwrap();
        plan.record_checked_route(caller, expression, &first, false, span)
            .unwrap();

        // An identical revisit is allowed; it may not change the selected dispatch mode.
        plan.record_checked_route(caller, expression, &first, false, span)
            .unwrap();
        for conflict in [&first, &second] {
            assert_eq!(
                plan.record_checked_route(caller, expression, conflict, true, span),
                Err(lowering_error(LoweringErrorKind::InvalidModel, span))
            );
        }
        let recorded = plan.call_site(caller, expression).unwrap();
        assert_eq!(recorded.key(), first.key());
        assert_eq!(recorded.dispatch_key(), None);

        // Compare the entire plan including its arena, not just the route getter.
        let mut original = UnitCallablePlan::default();
        let caller = original.reserve_source(root.key(), span).unwrap();
        original
            .record_checked_route(caller, expression, &first, false, span)
            .unwrap();
        assert_eq!(plan, original);
    }
}
