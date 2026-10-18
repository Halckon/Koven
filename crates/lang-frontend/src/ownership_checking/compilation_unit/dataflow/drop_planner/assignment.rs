//! Assignment cleanup preserves the old owner through the RHS and transfers its replacement.

use crate::{
    ast::ExpressionId,
    parser::AssignmentOperator,
    type_checking::{Copyability, UnitAggregateProjectionKind, UnitExpressionId},
};

use super::{
    DropExpressionUse, DropPlanner, OwnedValue, OwnershipCheckingError, PlannerDropFact,
    PlannerDropPoint, PlannerDropTarget, ValueState,
};

impl DropPlanner<'_, '_> {
    pub(super) fn assignment(
        &mut self,
        id: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if self
            .checker
            .typed
            .map_put(self.checker.unit_expression(target))
            .is_some()
        {
            return self.map_assignment(id, target, state);
        }
        if let Some(descriptor) = self.checker.element_place_descriptor(target)? {
            self.expression(
                descriptor.receiver().expression(),
                DropExpressionUse::Place,
                state,
            )?;
            self.expression(
                descriptor.index().expression(),
                DropExpressionUse::Read,
                state,
            )?;
            self.expression(value, DropExpressionUse::Consume, state)?;
            if self.checker.typed.copyability(descriptor.element_type()) == Copyability::MoveOnly {
                self.push_fact(PlannerDropFact::new(
                    PlannerDropPoint::AfterReplacement(id),
                    PlannerDropTarget::ReplacedElement(id),
                    self.checker.parsed.ast().expressions().get(target)?.span(),
                ));
            }
            if let Some(temporary) = self
                .checker
                .temporary_element_owner(target)?
                .map(UnitExpressionId::expression)
            {
                self.push_fact(PlannerDropFact::new(
                    PlannerDropPoint::AfterExpression(id),
                    PlannerDropTarget::Temporary(temporary),
                    self.checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(temporary)?
                        .span(),
                ));
            } else if let Some(place) = self.checker.place(target)?
                && !self.liveness.expression_after[id.index()].contains(&place.root())
            {
                self.drop_named_asap(PlannerDropPoint::AfterExpression(id), place.root(), state);
            }
            return Ok(true);
        }
        let root = self
            .checker
            .place(target)?
            .filter(|place| place.is_root())
            .map(|place| place.root());
        let closure = if operator == AssignmentOperator::Assign {
            self.closure_origin(value, state)?
        } else {
            None
        };
        if let Some(root) = root {
            state.replacements.push(root);
        }
        let continues = self.expression(value, DropExpressionUse::Consume, state)?;
        if root.is_some() {
            state.replacements.pop();
        }
        if !continues {
            return Ok(false);
        }
        let assignment = UnitExpressionId::new(self.checker.source_unit, id);
        let assignment_target = UnitExpressionId::new(self.checker.source_unit, target);
        let assignment_value = UnitExpressionId::new(self.checker.source_unit, value);
        if operator == AssignmentOperator::Assign
            && self
                .checker
                .typed
                .assignment(assignment)
                .is_some_and(|descriptor| {
                    descriptor.expression() == assignment
                        && descriptor.target() == assignment_target
                        && descriptor.value() == assignment_value
                        && descriptor.operator() == AssignmentOperator::Assign
                        && descriptor.falls_through()
                })
            && let Some(projection) = self.checker.typed.aggregate_projection(assignment_target)
            && projection.kind() == UnitAggregateProjectionKind::Field
            && self.checker.typed.copyability(projection.ty()) == Copyability::MoveOnly
        {
            self.push_fact(PlannerDropFact::new(
                PlannerDropPoint::BeforeReplacement(id),
                PlannerDropTarget::ReplacedField {
                    assignment: id,
                    field: projection.field(),
                },
                self.checker.parsed.ast().expressions().get(target)?.span(),
            ));
        }
        if let Some(place) = self.checker.place(target)?
            && place.is_root()
        {
            let symbol = place.root();
            if operator != AssignmentOperator::Assign {
                self.expression(target, DropExpressionUse::Read, state)?;
            }
            let old = state.remove_value(symbol);
            let old_closure = state.closures.remove(&symbol);
            if self.checker.is_move_only_variable(symbol) {
                state.insert(OwnedValue {
                    symbol,
                    origin: self.checker.parsed.ast().expressions().get(id)?.span(),
                    declaration: self.checker.symbol_span(symbol)?,
                    scope_depth: old
                        .map(|value| value.scope_depth)
                        .or_else(|| self.binding_depths.get(&symbol).copied())
                        .unwrap_or(self.scope_depth),
                });
                if let Some(closure) = closure {
                    state.closures.insert(symbol, closure);
                }
            }
            // The replacement can borrow the same source as the old environment.
            // Bind it before old-capture cleanup considers that source's last use.
            if let Some(old) = old {
                self.drop_owned_value(
                    PlannerDropPoint::AfterExpression(value),
                    old,
                    old_closure,
                    state,
                );
            }
            if self.checker.is_move_only_variable(symbol)
                && !self.liveness.expression_after[id.index()].contains(&symbol)
            {
                self.drop_named_asap(PlannerDropPoint::AfterExpression(id), symbol, state);
            }
        }
        Ok(true)
    }
}
