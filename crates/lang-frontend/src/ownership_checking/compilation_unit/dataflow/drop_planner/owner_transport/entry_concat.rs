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
        literal_concat_operands(self.checker, expression)
    }

    // Called only after string_binary completed the two actual operand traversals
    // and published their AfterBinaryOperands cleanup facts.
    pub(in super::super) fn completed_entry_concat(
        &mut self,
        expression: ExpressionId,
    ) -> Option<UnitOwnerId> {
        let [left, right] = self.entry_concat_operands(expression)?;
        if !self.concat_operands_cleaned(expression, [left, right]) {
            return None;
        }
        self.define_owner(UnitOwnerOrigin::StringConcat {
            expression: self.checker.unit_expression(expression),
            left: self.checker.unit_expression(left),
            right: self.checker.unit_expression(right),
        })
    }
    // Reused by the combined scalar-entry admission after the origin was published.
    fn concat_operands_cleaned(
        &self,
        expression: ExpressionId,
        operands: [ExpressionId; 2],
    ) -> bool {
        let Some(observation) = self.owner_observation.as_ref() else {
            return false;
        };
        operands.into_iter().all(|operand| {
            self.facts.iter().any(|fact| {
                fact.point() == PlannerDropPoint::AfterBinaryOperands(expression)
                    && fact.target() == PlannerDropTarget::Temporary(operand)
            }) && observation
                .definitions
                .iter()
                .any(|d| d.origin == UnitOwnerOrigin::String(self.checker.unit_expression(operand)))
        })
    }
    pub(in super::super) fn completed_concat_origin(&self, origin: UnitOwnerOrigin) -> bool {
        let UnitOwnerOrigin::StringConcat {
            expression,
            left,
            right,
        } = origin
        else {
            return false;
        };
        if [expression, left, right]
            .iter()
            .any(|id| id.source_unit() != self.checker.source_unit)
        {
            return false;
        }
        literal_concat_operands(self.checker, expression.expression())
            == Some([left.expression(), right.expression()])
            && self.concat_operands_cleaned(
                expression.expression(),
                [left.expression(), right.expression()],
            )
    }
}

// Shape only. Actual owner admission additionally checks entry position and completion.
pub(super) fn literal_concat_operands(
    checker: &Checker<'_>,
    expression: ExpressionId,
) -> Option<[ExpressionId; 2]> {
    let ast = checker.parsed.ast();
    let Expression::Binary {
        left,
        operator: BinaryOperator::Add,
        right,
        ..
    } = ast.expressions().get(expression).ok()?.payload()
    else {
        return None;
    };
    for operand in [expression, *left, *right] {
        if checker
            .typed
            .expression_type(checker.unit_expression(operand))
            .and_then(|ty| checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        {
            return None;
        }
    }
    for operand in [*left, *right] {
        let Expression::String { parts } = ast.expressions().get(operand).ok()?.payload() else {
            return None;
        };
        if parts
            .iter()
            .any(|p| matches!(p, StringPart::Interpolation { .. }))
        {
            return None;
        }
    }
    Some([*left, *right])
}
