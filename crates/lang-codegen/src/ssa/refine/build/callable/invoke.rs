//! Form a temporary callee before evaluating operands and invoke its concrete ABI.
use super::*;
use crate::ssa::refine::body::RTerm;

impl Builder<'_, '_> {
    pub(in crate::ssa::refine::build) fn form_literal_callee(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let Expression::Call { callee, .. } = self.node(expression)?.0 else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let root = self.ungrouped_expr(callee)?;
        let (payload, span) = self.node(root)?;
        if matches!(payload, Expression::Lambda { .. }) {
            return self.form_callable(root, span).map(Some);
        }
        Ok(None)
    }

    /// Calls a view parameter (`CallableInvokeView`) or a local callable (`CallableInvoke`).
    pub(in crate::ssa::refine::build) fn invoke_callable(
        &mut self,
        expression: ExpressionId,
        arguments: Vec<Slot>,
        literal_callee: Option<Slot>,
        return_type: UnitTypeId,
        span: Span,
    ) -> Result<(Lowered, Option<ExpressionId>), LoweringError> {
        let (op, consumed_callee) = if let Some(callable) = literal_callee {
            (
                ROp::CallableInvoke {
                    callable,
                    arguments,
                },
                None,
            )
        } else {
            self.named_callable_operation(expression, arguments, span)?
        };
        let result = if self.is_nothing(return_type) {
            self.push(op, Vec::new(), span)?;
            self.terminate(RTerm::Unreachable, span)?;
            Lowered::Diverged
        } else if self.is_unit(return_type) {
            self.push(op, Vec::new(), span)?;
            Lowered::Unit
        } else {
            Lowered::Value(self.typed_value(op, expression)?)
        };
        Ok((result, consumed_callee))
    }

    fn named_callable_operation(
        &self,
        expression: ExpressionId,
        arguments: Vec<Slot>,
        span: Span,
    ) -> Result<(ROp, Option<ExpressionId>), LoweringError> {
        let Expression::Call { callee, .. } = self.node(expression)?.0 else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let root = self.ungrouped_expr(callee)?;
        let (Expression::Name, callee_span) = self.node(root)? else {
            return Err(error(LoweringErrorKind::MissingFact, self.span(callee)));
        };
        let symbol = self.reference(callee_span)?;
        if let Some(&view) = self.borrowed.get(&symbol) {
            Ok((ROp::CallableInvokeView { view, arguments }, None))
        } else if let Some(&callable) = self.bindings.get(&symbol) {
            Ok((
                ROp::CallableInvoke {
                    callable,
                    arguments,
                },
                Some(callee),
            ))
        } else {
            Err(error(LoweringErrorKind::MissingFact, callee_span))
        }
    }
}
