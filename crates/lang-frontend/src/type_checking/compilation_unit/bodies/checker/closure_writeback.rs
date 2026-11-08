//! Private recovery facts for directly identified closure values.

use super::*;

pub(super) type ClosureEffects = BTreeMap<UnitExpressionId, ClosureEffect>;
pub(super) type ClosureOrigins = BTreeMap<UnitSymbolId, UnitExpressionId>;

#[derive(Clone)]
pub(super) struct ClosureEffect {
    captures: BTreeSet<UnitSymbolId>,
    returns: bool,
}

impl BodyChecker<'_> {
    pub(super) fn finish_closure_effects(
        &mut self,
        origin: UnitExpressionId,
        returns: call_writeback::PendingCallableReturns,
        before: &FlowVersions,
        trusted_exit: bool,
    ) {
        let (mut captures, returns) = returns.captured_exits();
        // A hard body error can fabricate an abrupt recovery edge.
        let returns = returns || !trusted_exit;
        captures.retain(|&symbol| {
            before.tracks_mutable(FlowKey::Symbol(symbol))
                && !before.mutable_is_pending(symbol)
                && self.names.source_units()[symbol.source_unit().index()]
                    .resolution()
                    .symbols()
                    .get(symbol.symbol().index())
                    .is_some_and(|symbol| symbol.kind() == SymbolKind::Variable)
        });
        if returns && captures.is_empty() {
            self.closure_effects.remove(&origin);
        } else {
            self.closure_effects
                .insert(origin, ClosureEffect { captures, returns });
        }
    }

    pub(super) fn note_closure_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: UnitExpressionId,
    ) {
        if let Some(origin) = self.closure_origin(initializer) {
            self.closure_origins.insert(symbol, origin);
        } else {
            self.closure_origins.remove(&symbol);
        }
    }

    fn closure_origin(&self, mut expression: UnitExpressionId) -> Option<UnitExpressionId> {
        let source = expression.source_unit();
        loop {
            let node = self
                .file(source)
                .ast()
                .expressions()
                .get(expression.expression())
                .ok()?;
            match node.payload() {
                Expression::Group { expression: inner } => {
                    expression = UnitExpressionId::new(source, *inner);
                }
                Expression::Lambda { .. } => {
                    return self
                        .closure_effects
                        .contains_key(&expression)
                        .then_some(expression);
                }
                Expression::Name => {
                    let FlowKey::Symbol(symbol) =
                        self.flow_root(source, expression.expression())?
                    else {
                        return None;
                    };
                    return self.closure_origins.get(&symbol).copied();
                }
                _ => return None,
            }
        }
    }

    pub(super) fn known_closure_returns(&self, callee: UnitExpressionId) -> bool {
        self.closure_origin(callee)
            .and_then(|origin| self.closure_effects.get(&origin))
            .is_none_or(|effect| effect.returns)
    }

    pub(super) fn apply_closure_effects(&mut self, callee: UnitExpressionId) {
        if let Some(origin) = self.closure_origin(callee)
            && let Some(effect) = self.closure_effects.get(&origin)
        {
            self.flow_versions
                .mark_pending(effect.captures.iter().copied().map(FlowKey::Symbol));
        }
    }
}
