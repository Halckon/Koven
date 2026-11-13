//! Narrow direct-entry producer. This never invokes literal-only loop hooks.
use super::*;

impl DropPlanner<'_, '_> {
    pub(super) fn entry_concat_operands(
        &self,
        expression: ExpressionId,
    ) -> Option<[ExpressionId; 2]> {
        if self.owner_controls.current().is_some() || self.owner_branch_depth != 0 {
            return None;
        }
        let observation = self.owner_observation.as_ref()?;
        if observation
            .definitions
            .iter()
            .any(|d| matches!(d.origin, UnitOwnerOrigin::StringConcat { .. }))
        {
            return None;
        }
        let UnitOwnerCallable::Function(function) = observation.callable else {
            return None;
        };
        let ast = self.checker.parsed.ast();
        let Item::Function { form, .. } = ast.items().get(function.item()).ok()?.payload() else {
            return None;
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return None,
        };
        let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
            return None;
        };
        // Only a direct initializer in the straight-line local-declaration prefix.
        let mut found = false;
        for statement in elements {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*statement).ok()?.payload()
            else {
                break;
            };
            let Item::Variable {
                initializer, kind, ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if *initializer == expression {
                found = !matches!(kind, crate::parser::VariableKind::BorrowVal(_));
                break;
            }
        }
        if !found {
            return None;
        }
        let Expression::Binary {
            left,
            operator: BinaryOperator::Add,
            right,
            ..
        } = ast.expressions().get(expression).ok()?.payload()
        else {
            return None;
        };
        for operand in [*left, *right] {
            let Expression::String { parts } = ast.expressions().get(operand).ok()?.payload()
            else {
                return None;
            };
            if parts
                .iter()
                .any(|p| matches!(p, StringPart::Interpolation { .. }))
                || !self.is_string_expression(operand)
            {
                return None;
            }
        }
        self.is_string_expression(expression)
            .then_some([*left, *right])
    }

    // Called only after string_binary completed the two actual operand traversals
    // and published their AfterBinaryOperands cleanup facts.
    pub(in super::super) fn completed_entry_concat(
        &mut self,
        expression: ExpressionId,
    ) -> Option<UnitOwnerId> {
        let [left, right] = self.entry_concat_operands(expression)?;
        for operand in [left, right] {
            if !self.facts.iter().any(|fact| {
                fact.point() == PlannerDropPoint::AfterBinaryOperands(expression)
                    && fact.target() == PlannerDropTarget::Temporary(operand)
            }) {
                return None;
            }
            if !self
                .owner_observation
                .as_ref()?
                .definitions
                .iter()
                .any(|d| d.origin == UnitOwnerOrigin::String(self.checker.unit_expression(operand)))
            {
                return None;
            }
        }
        self.define_owner(UnitOwnerOrigin::StringConcat {
            expression: self.checker.unit_expression(expression),
            left: self.checker.unit_expression(left),
            right: self.checker.unit_expression(right),
        })
    }
}
