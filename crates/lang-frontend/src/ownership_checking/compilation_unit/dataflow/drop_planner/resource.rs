//! Fail closed when the unit planner cannot transport a conditional resource obligation.
use super::{
    DropPlanner, ExpressionId, OwnershipDeferredReason, PlannerDropPoint,
    UnitOwnershipDeferredFact, UnitSymbolId, ValueState, merge_value_states,
};

impl DropPlanner<'_, '_> {
    pub(super) fn is_asap_owner(&self, symbol: UnitSymbolId, state: &ValueState) -> bool {
        let Some(ty) = self.checker.typed.symbol_type(symbol) else {
            return false;
        };
        if state.closures.contains_key(&symbol) {
            return state.closure_asap.get(&symbol) == Some(&true);
        }
        if let Some(resource) = self.checker.typed.is_resource_type(ty) {
            return !resource;
        }
        let mut environment = ty;
        while let Some(
            crate::type_checking::UnitTypeKind::Nullable(inner)
            | crate::type_checking::UnitTypeKind::EnumCase { root: inner, .. },
        ) = self.checker.typed.types().get(environment)
        {
            environment = *inner;
        }
        if !matches!(
            self.checker.typed.types().get(environment),
            Some(crate::type_checking::UnitTypeKind::Function { .. })
        ) {
            return false;
        }
        if self
            .checker
            .typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.nominal())
            .all(|nominal| !nominal.has_deinit())
        {
            return true;
        }
        false
    }

    /// Read the replacement environment before Consume removes its captured owners.
    /// Facts remain per current binding, so repeated lambda sites cannot overwrite aliases.
    pub(super) fn closure_value_asap(
        &self,
        expression: ExpressionId,
        state: &ValueState,
    ) -> Result<Option<bool>, super::OwnershipCheckingError> {
        let node = self.checker.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            crate::parser::Expression::Group { expression } => {
                self.closure_value_asap(*expression, state)
            }
            crate::parser::Expression::Name => Ok(self
                .checker
                .reference_symbol(node.span())
                .filter(|symbol| state.closures.contains_key(symbol))
                .map(|symbol| state.closure_asap.get(&symbol) == Some(&true))),
            crate::parser::Expression::Lambda { .. } => {
                let pure = self.checker.captures_of(expression).all(|capture| {
                    if capture.mode() != crate::ownership_checking::ClosureCaptureMode::Owned {
                        return true;
                    }
                    if let crate::ownership_checking::UnitClosureCaptureSource::Symbol(symbol) =
                        capture.source()
                        && state.closures.contains_key(&symbol)
                    {
                        return state.closure_asap.get(&symbol) == Some(&true);
                    }
                    // An ordinary function type cannot prove its unknown environment pure.
                    !matches!(
                        self.checker.typed.types().get(capture.ty()),
                        Some(crate::type_checking::UnitTypeKind::Function { .. })
                    ) && self.checker.typed.is_resource_type(capture.ty()) == Some(false)
                });
                Ok(Some(pure))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn drop_named_asap(
        &mut self,
        point: PlannerDropPoint,
        symbol: UnitSymbolId,
        state: &mut ValueState,
    ) {
        if self.is_asap_owner(symbol, state) {
            self.drop_named(point, symbol, state);
        }
    }

    pub(super) fn defer_resource(&mut self, expression: ExpressionId) {
        let fact = UnitOwnershipDeferredFact::new(
            self.checker.unit_expression(expression),
            OwnershipDeferredReason::ResourceLifetime,
        );
        if !self.resource_deferred.contains(&fact) {
            self.resource_deferred.push(fact);
        }
    }

    pub(super) fn take_named(
        &mut self,
        expression: ExpressionId,
        symbol: UnitSymbolId,
        state: &mut ValueState,
    ) {
        if !self.is_asap_owner(symbol, state)
            && let Some(&depth) = self.loop_boundaries.last()
            && state
                .values
                .iter()
                .any(|value| value.symbol == symbol && value.scope_depth <= depth)
        {
            self.defer_resource(expression);
        }
        state.take(symbol);
    }

    pub(super) fn merge_resource_states(
        &mut self,
        expression: ExpressionId,
        states: Vec<ValueState>,
    ) -> ValueState {
        if states.iter().any(|state| {
            state.values.iter().any(|value| {
                !self.is_asap_owner(value.symbol, state)
                    && states
                        .iter()
                        .any(|state| state.position(value.symbol).is_none())
            })
        }) {
            self.defer_resource(expression);
        }
        merge_value_states(states)
    }
}
