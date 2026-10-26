//! Subset rules for field replacement (SPEC-0357 step 2): `local.f = v` on a plain class
//! owner held by a known local. They mirror `build/field.rs`.
use lang_frontend::{
    ast::ExpressionId,
    parser::{AssignmentOperator, Expression},
    type_checking::{UnitAggregateProjectionKind, UnitAggregateProjectionReceiver},
};

use super::Checker;

impl Checker<'_, '_> {
    /// Whether the assignment target is a field projection (not a binding).
    pub(super) fn is_field_target(&self, target: ExpressionId) -> bool {
        self.inputs
            .typed
            .aggregate_projection(self.id(target))
            .is_some()
    }

    /// `owner.f = v`: the owner is a known local plain class, `f` one of its scalar or
    /// String fields; the RHS is in the subset.
    pub(super) fn field_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
    ) -> bool {
        self.migrated = true;
        let typed = self.inputs.typed;
        let (assignment_id, target_id) = (self.id(assignment), self.id(target));
        let (Some(descriptor), Some(projection)) = (
            typed.assignment(assignment_id),
            typed.aggregate_projection(target_id),
        ) else {
            return false;
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return false;
        };
        let field_type = projection.ty();
        if descriptor.target() != target_id
            || descriptor.value() != self.id(value)
            || descriptor.operator() != AssignmentOperator::Assign
            || descriptor.target_type() != field_type
            || projection.kind() != UnitAggregateProjectionKind::Field
            || receiver.source_unit() != self.inputs.source_unit
            || !(self.scalar(field_type) || self.string(field_type))
        {
            return false;
        }
        let Ok(node) = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(receiver.expression())
        else {
            return false;
        };
        let Expression::Name = node.payload() else {
            return false;
        };
        let Some(symbol) = self.reference(receiver.expression()) else {
            return false;
        };
        let owner_ok = self.known.contains(&symbol)
            && self.parameter_mode(symbol).is_none()
            && typed.non_null_use(receiver).is_none()
            && typed.symbol_type(symbol).is_some_and(|owner| {
                typed.expression_type(receiver) == Some(owner)
                    && self.class(owner)
                    && self.class_signature(owner).is_some_and(|nominal| {
                        nominal
                            .fields()
                            .iter()
                            .any(|field| field.symbol() == projection.field())
                    })
            });
        if !owner_ok || !self.expression(value) {
            return false;
        }
        self.replacements.insert(assignment_id);
        true
    }
}
