//! Subset rules for field replacement (SPEC-0358): `owner.f = v` on a plain class owner held
//! by a known local, an Inout parameter or the Inout receiver. They mirror `build/field.rs`.
use lang_frontend::{
    ast::ExpressionId,
    parser::AssignmentOperator,
    type_checking::{ParameterMode, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver},
};

use super::{Checker, receiver::Owner};

impl Checker<'_, '_> {
    /// Whether the assignment target is a field projection (not a binding).
    pub(super) fn is_field_target(&self, target: ExpressionId) -> bool {
        self.inputs
            .typed
            .aggregate_projection(self.id(target))
            .is_some()
    }

    /// `owner.f = v`: `f` is a scalar or String field of the owner's plain class; the RHS is
    /// in the subset.
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
        let field_type = self.c(projection.ty());
        if descriptor.target() != target_id
            || descriptor.value() != self.id(value)
            || descriptor.operator() != AssignmentOperator::Assign
            || self.c(descriptor.target_type()) != field_type
            || projection.kind() != UnitAggregateProjectionKind::Field
            || !(self.scalar(field_type) || self.string(field_type))
        {
            return false;
        }
        let owner = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                Owner::Expression(receiver.expression())
            }
            UnitAggregateProjectionReceiver::This(declaration) => Owner::This(declaration),
        };
        let owner_ok = self
            .owner_type(owner, ParameterMode::Inout)
            .is_some_and(|owner| self.class_field(owner, projection.field()));
        if !owner_ok || !self.expression(value) {
            return false;
        }
        self.replacements.insert(assignment_id);
        true
    }
}
