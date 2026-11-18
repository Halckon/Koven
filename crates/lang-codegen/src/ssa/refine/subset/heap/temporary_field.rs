//! Admit only a complete owned temporary field read and its exact parent cleanup.
use super::*;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
};

impl Checker<'_, '_> {
    pub(super) fn owned_factory_result(&self, expression: ExpressionId) -> bool {
        use lang_frontend::type_checking::{CallableResultSource, UnitCallTarget};
        self.inputs
            .typed
            .call(self.id(expression))
            .is_some_and(|call| {
                matches!(call.target(), UnitCallTarget::Declaration(_))
                    && call.receiver().is_none()
                    && call.result_source() == CallableResultSource::Owned
                    && self
                        .expression_type(expression)
                        .is_some_and(|ty| self.class(ty))
            })
    }

    /// Preserve the existing factory-prefix cleanup proof before the generic
    /// non-completing receiver path. None leaves all other main routes unchanged.
    /// Once matched, failure is final: expression checking may mutate this checker.
    pub(super) fn non_dispatching_factory_projection(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
    ) -> Option<bool> {
        let id = self.id(expression);
        let receiver_id = self.id(receiver);
        let projection = self.inputs.typed.aggregate_projection(id)?;
        if projection.expression() != id
            || projection.kind() != UnitAggregateProjectionKind::Field
            || projection.receiver() != UnitAggregateProjectionReceiver::Expression(receiver_id)
            || self.expression_type_of(id) != Some(self.c(projection.ty()))
            || !self.storage_type(self.c(projection.ty()))
            || self.copyability_of(self.c(projection.ty())) != Copyability::Copyable
            || self.inputs.typed.expression_falls_through(receiver_id) != Some(false)
        {
            return None;
        }
        let call = self.ungrouped(receiver);
        if !self.owned_factory_result(call)
            || self.inputs.owned.call_may_dispatch(self.id(call)) != Some(false)
        {
            return None;
        }
        let field = projection.field();
        Some(self.temporary_field_read(receiver, call, field))
    }

    pub(super) fn temporary_field_read(
        &mut self,
        receiver_expression: ExpressionId,
        constructor: ExpressionId,
        field: UnitSymbolId,
    ) -> bool {
        let Some(owner) = self.expression_type(receiver_expression) else {
            return false;
        };
        let constructor_id = self.id(constructor);
        let receiver_id = self.id(receiver_expression);
        if self.inputs.typed.construction(constructor_id).is_some()
            && self.inputs.typed.expression_falls_through(receiver_id) == Some(false)
        {
            if !self.class_field(owner, field)
                || self.expression_type(constructor) != Some(owner)
                || self
                    .inputs
                    .owned
                    .drops()
                    .iter()
                    .any(|fact| fact.target() == UnitDropTarget::Temporary(constructor_id))
                || !self.terminated_construction(constructor)
            {
                return false;
            }
            self.migrated = true;
            return true;
        }
        if self.owned_factory_result(constructor)
            && self.inputs.typed.expression_falls_through(receiver_id) == Some(false)
            && self.inputs.owned.call_may_dispatch(constructor_id) == Some(false)
        {
            if !self.class_field(owner, field)
                || self.expression_type(constructor) != Some(owner)
                || self
                    .inputs
                    .owned
                    .drops()
                    .iter()
                    .any(|fact| fact.target() == UnitDropTarget::Temporary(constructor_id))
                || !self.expression(constructor)
            {
                return false;
            }
            self.non_dispatching_field_calls.insert(constructor_id);
            self.migrated = true;
            return true;
        }
        let cleanup = self
            .inputs
            .owned
            .drops()
            .iter()
            .filter(|fact| {
                fact.point() == UnitDropPoint::AfterExpression(receiver_id)
                    && fact.target() == UnitDropTarget::Temporary(constructor_id)
                    && self
                        .inputs
                        .parsed
                        .ast()
                        .expressions()
                        .get(constructor)
                        .is_ok_and(|node| fact.value_origin() == node.span())
            })
            .count();
        if cleanup != 1
            || !self.class_field(owner, field)
            || self.expression_type(constructor) != Some(owner)
            || !self.expression(constructor)
        {
            return false;
        }
        self.migrated = true;
        true
    }
}
