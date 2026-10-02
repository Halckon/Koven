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
        state.closures.get(&symbol).is_some_and(|&closure| {
            self.checker.captures_of(closure).all(|capture| {
                capture.mode() != crate::ownership_checking::ClosureCaptureMode::Owned
                    || self.checker.typed.is_resource_type(capture.ty()) == Some(false)
            })
        })
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
