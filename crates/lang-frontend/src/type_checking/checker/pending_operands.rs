//! Preserve recovery reachability without changing ordinary type or diagnostic checking.
use super::*;
use crate::parser::{BinaryOperator, Expression, LiteralKind, StringPart};

pub(super) fn unconditional_operand(expression: &Expression) -> Option<ExpressionId> {
    match expression {
        Expression::Group { expression }
        | Expression::Cast { expression, .. }
        | Expression::TypeTest { expression, .. }
        | Expression::Prefix {
            operand: expression,
            ..
        }
        | Expression::NonNullAssert {
            operand: expression,
            ..
        }
        | Expression::Member {
            receiver: expression,
            ..
        }
        | Expression::Propagate {
            value: expression, ..
        } => Some(*expression),
        Expression::CallableReference { receiver, .. } => *receiver,
        _ => None,
    }
}

impl Checker<'_> {
    pub(super) fn note_pending_exit(&mut self, expression: ExpressionId, reachable: bool) {
        if !reachable {
            self.iteration_recovery
                .abrupt_expressions
                .insert(expression.index());
        }
    }

    pub(super) fn pending_checked_exit(&self, expression: ExpressionId) -> bool {
        let falls_through = self.expression_types[expression.index()]
            .is_none_or(|ty| !self.is_builtin(ty, BuiltinType::Nothing));
        self.pending_expression_exit(expression, falls_through)
    }

    pub(super) fn check_pending_operand(
        &mut self,
        argument: ExpressionId,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
        reachable: &mut bool,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let result = self.check_pending_path(*reachable, |checker| {
            let recovery = &mut checker.iteration_recovery;
            let Some(entry) = recovery.deferred_argument_entries.remove(&argument.index()) else {
                return checker.check_expression(argument, expected, expected_span);
            };
            // Inference can check subsequent operands before an expected lambda type exists.
            let following = std::mem::replace(&mut recovery.bindings, entry);
            let result = checker.check_expression(argument, expected, expected_span);
            let lambda_exit =
                std::mem::replace(&mut checker.iteration_recovery.bindings, following);
            checker.iteration_recovery.bindings.extend(
                lambda_exit
                    .intersection(&checker.captured_mutable_symbols)
                    .copied(),
            );
            result
        })?;
        *reachable &= self.pending_expression_exit(argument, result.falls_through);
        Ok(result)
    }

    pub(super) fn check_eager_binary_operands(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> Result<(ExprCheck, ExprCheck), TypeCheckingError> {
        let equality = matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual);
        let left_null = equality
            && matches!(
                self.ast().expressions().get(left)?.payload(),
                Expression::Literal(LiteralKind::Null)
            );
        let right_null = equality
            && matches!(
                self.ast().expressions().get(right)?.payload(),
                Expression::Literal(LiteralKind::Null)
            );
        let (left_result, right_result) = if left_null && !right_null {
            let right_result = self.check_expression(right, None, None)?;
            let expected = self.null_comparison_expected(right, right_result.ty);
            (self.check_expression(left, expected, None)?, right_result)
        } else if right_null && !left_null {
            let left_result = self.check_expression(left, None, None)?;
            let expected = self.null_comparison_expected(left, left_result.ty);
            (left_result, self.check_expression(right, expected, None)?)
        } else {
            let mut reachable = true;
            let left = self.check_pending_operand(left, None, None, &mut reachable)?;
            let right = self.check_pending_operand(right, None, None, &mut reachable)?;
            (left, right)
        };
        // A literal null may be typed after the other operand to infer its context.
        let reachable = self.pending_expression_exit(left, left_result.falls_through)
            && self.pending_expression_exit(right, right_result.falls_through);
        self.note_pending_exit(expression, reachable);
        Ok((left_result, right_result))
    }

    pub(super) fn check_pending_receiver_operand(
        &mut self,
        receiver: ExpressionId,
        operand: ExpressionId,
        expected: TypeId,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let mut reachable = self.pending_checked_exit(receiver);
        self.check_pending_operand(operand, Some(expected), None, &mut reachable)
    }

    pub(super) fn check_string_operands(
        &mut self,
        id: ExpressionId,
        span: Span,
        parts: &[StringPart],
    ) -> Result<ExprCheck, TypeCheckingError> {
        if let Some(diagnostic) =
            crate::type_checking::string_length::check_literal(self.sources, span, parts)?
        {
            self.diagnostics.push(diagnostic);
        }
        let mut reachable = true;
        for part in parts {
            if let StringPart::Interpolation { expression, .. } = part {
                self.check_pending_operand(*expression, None, None, &mut reachable)?;
            }
        }
        self.note_pending_exit(id, reachable);
        Ok(ExprCheck {
            ty: self.builtin(BuiltinType::String),
            falls_through: true,
        })
    }
}
