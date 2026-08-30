//! compilation-unit root name assignment 与 owner replacement lowering。

use lang_frontend::{
    ast::ExpressionId,
    parser::{AssignmentOperator, Expression},
    source::Span,
    type_checking::{Copyability, UnitExpressionId},
};

use super::{
    LoweredValue, UnitExpressionLowerer, lowering_error, scalar::is_integer_builtin, span_key,
};
use crate::ssa::{LoweringError, LoweringErrorKind, model::CheckedArithmeticOperator};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_assignment(
        &mut self,
        expression: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self.element_place_descriptor(target)?.is_some() {
            return self.lower_container_assignment(expression, target, operator, value, span);
        }
        let target_node = self
            .parsed
            .ast()
            .expressions()
            .get(target)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(target_node.payload(), Expression::Name) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                target_node.span(),
            ));
        }
        let symbol = self
            .references
            .get(&span_key(target_node.span()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, target_node.span()))?;
        let target_type = self
            .typed
            .types()
            .expression_type(UnitExpressionId::new(self.source_unit, target))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, target_node.span()))?;
        let target_ssa_type = self.expression_ssa_type(target, target_node.span())?;

        let assigned = match operator {
            AssignmentOperator::Assign => {
                let assigned = self.lower(value)?;
                if assigned != LoweredValue::Diverged {
                    self.require_matching_assignment_type(value, target_ssa_type, span)?;
                }
                assigned
            }
            AssignmentOperator::AddAssign
            | AssignmentOperator::SubtractAssign
            | AssignmentOperator::MultiplyAssign
            | AssignmentOperator::DivideAssign
            | AssignmentOperator::RemainderAssign => {
                if !self
                    .expression_builtin_type(target, target_node.span())?
                    .is_some_and(is_integer_builtin)
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        target_node.span(),
                    ));
                }
                let left = match self.bindings.get(&symbol).copied() {
                    Some(LoweredValue::Value(value)) => value,
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            target_node.span(),
                        ));
                    }
                };
                let right = match self.lower(value)? {
                    LoweredValue::Value(value) => value,
                    LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                };
                self.require_matching_assignment_type(value, target_ssa_type, span)?;
                self.checked(
                    assignment_operator(operator),
                    left,
                    right,
                    target_ssa_type,
                    span,
                )?
            }
        };
        if assigned == LoweredValue::Diverged {
            return Ok(assigned);
        }
        if let LoweredValue::Value(value_id) = assigned {
            self.transfer_owned_expression(value, value_id, span)?;
        }
        if self.typed.types().copyability(target_type) == Copyability::MoveOnly
            && self.bindings.contains_key(&symbol)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.bindings.insert(symbol, assigned);
        Ok(LoweredValue::Unit)
    }

    fn require_matching_assignment_type(
        &self,
        value: ExpressionId,
        target_type: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.expression_ssa_type(value, span)? != target_type {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }
}

fn assignment_operator(operator: AssignmentOperator) -> CheckedArithmeticOperator {
    match operator {
        AssignmentOperator::AddAssign => CheckedArithmeticOperator::Add,
        AssignmentOperator::SubtractAssign => CheckedArithmeticOperator::Subtract,
        AssignmentOperator::MultiplyAssign => CheckedArithmeticOperator::Multiply,
        AssignmentOperator::DivideAssign => CheckedArithmeticOperator::Divide,
        AssignmentOperator::RemainderAssign => CheckedArithmeticOperator::Remainder,
        AssignmentOperator::Assign => unreachable!("plain assignment has no arithmetic operator"),
    }
}
