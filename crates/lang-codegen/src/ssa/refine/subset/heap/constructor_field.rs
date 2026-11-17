//! Admit only a complete direct-constructor field read and its exact parent cleanup.
use super::*;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
};

impl Checker<'_, '_> {
    pub(super) fn constructor_field_read(
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
