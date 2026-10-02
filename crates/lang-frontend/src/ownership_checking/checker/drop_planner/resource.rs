//! Resource lifetime policy is separate from ordinary last-use liveness.
use super::{DropPlanner, DropPoint, ExpressionId, SymbolId, ValueState};

impl DropPlanner<'_, '_> {
    /// Function types omit environment fields; use capture facts when resource classes exist.
    /// In a closed file without any resource declaration, an environment cannot own one.
    pub(super) fn is_asap_owner(&self, symbol: SymbolId, state: &ValueState) -> bool {
        let Some(ty) = self.checker.typed.symbol_type(symbol) else {
            return false;
        };
        if let Some(resource) = self.checker.typed.is_resource_type(ty) {
            return !resource;
        }
        let mut environment = ty;
        while let Some(
            crate::type_checking::TypeKind::Nullable(inner)
            | crate::type_checking::TypeKind::EnumCase { root: inner, .. },
        ) = self.checker.typed.types().get(environment)
        {
            environment = *inner;
        }
        if !matches!(
            self.checker.typed.types().get(environment),
            Some(crate::type_checking::TypeKind::Function { .. })
        ) {
            return false;
        }
        if self
            .checker
            .typed
            .nominals()
            .iter()
            .all(|nominal| !nominal.has_deinit())
        {
            return true;
        }
        state.closures.get(&symbol).is_some_and(|origins| {
            !origins.is_empty()
                && origins.iter().all(|origin| {
                    self.is_memory_closure(origin, &mut std::collections::BTreeSet::new())
                })
        })
    }

    fn is_memory_closure(
        &self,
        origin: &super::ClosureOrigin,
        seen: &mut std::collections::BTreeSet<usize>,
    ) -> bool {
        if !seen.insert(origin.closure.index()) {
            return true;
        }
        self.checker.captures_of(origin.closure).all(|capture| {
            if capture.mode() != crate::ownership_checking::ClosureCaptureMode::Owned {
                return true;
            }
            match self.checker.typed.is_resource_type(capture.ty()) {
                Some(resource) => !resource,
                None => {
                    let nested = origin
                        .captured
                        .iter()
                        .filter(|nested| nested.captured_from == Some(capture.source()))
                        .collect::<Vec<_>>();
                    !nested.is_empty()
                        && nested
                            .into_iter()
                            .all(|nested| self.is_memory_closure(nested, seen))
                }
            }
        })
    }

    pub(super) fn drop_named_asap(
        &mut self,
        point: DropPoint,
        symbol: SymbolId,
        state: &mut ValueState,
    ) {
        if self.is_asap_owner(symbol, state) {
            self.drop_named(point, symbol, state);
        }
    }

    pub(super) fn take_named(
        &mut self,
        expression: ExpressionId,
        symbol: SymbolId,
        state: &mut ValueState,
    ) {
        if !self.is_asap_owner(symbol, state)
            && let Some(&depth) = self.loop_boundaries.last()
            && state
                .values
                .iter()
                .any(|value| value.symbol == symbol && value.scope_depth <= depth)
        {
            self.resource_lifetime.get_or_insert(expression);
        }
        state.take(symbol);
    }
}
