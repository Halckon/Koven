//! Null literal has no evaluation effects; only the completed owning operand needs cleanup.
use super::*;
use crate::parser::LiteralKind;

impl DropPlanner<'_, '_> {
    pub(super) fn drop_nullable_comparison_temporary(
        &mut self,
        binary: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> Result<(), OwnershipCheckingError> {
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
            return Ok(());
        }
        let left = self.null_comparison_leaf(left)?;
        let right = self.null_comparison_leaf(right)?;
        let expressions = self.checker.parsed.ast().expressions();
        let operand = if matches!(
            expressions.get(right)?.payload(),
            Expression::Literal(LiteralKind::Null)
        ) {
            left
        } else if matches!(
            expressions.get(left)?.payload(),
            Expression::Literal(LiteralKind::Null)
        ) {
            right
        } else {
            return Ok(());
        };
        // Contextual typing can make null itself a MoveOnly nullable temporary.
        if matches!(
            expressions.get(operand)?.payload(),
            Expression::Literal(LiteralKind::Null)
        ) {
            return Ok(());
        }
        let Some(UnitTypeKind::Nullable(inner)) = self
            .checker
            .typed
            .expression_type(self.checker.unit_expression(operand))
            .and_then(|ty| self.checker.typed.types().get(ty))
        else {
            return Ok(());
        };
        let mut owner = operand;
        if matches!(
            self.checker.typed.types().get(*inner),
            Some(
                UnitTypeKind::Builtin(crate::type_checking::BuiltinType::String)
                    | UnitTypeKind::TypeParameter(_)
            )
        ) && let Some(parent) = self.checker.temporary_field_owner(operand)?
        {
            owner = self.null_comparison_leaf(parent.expression())?;
        }
        if !self.is_move_only_temporary(owner) {
            return Ok(());
        }
        let node = expressions.get(owner)?;
        self.push_fact(PlannerDropFact::new(
            PlannerDropPoint::AfterBinaryOperands(binary),
            PlannerDropTarget::Temporary(owner),
            node.span(),
        ));
        Ok(())
    }

    fn null_comparison_leaf(
        &self,
        mut expression: ExpressionId,
    ) -> Result<ExpressionId, OwnershipCheckingError> {
        while let Expression::Group { expression: inner } = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(expression)?
            .payload()
        {
            expression = *inner;
        }
        Ok(expression)
    }
}
