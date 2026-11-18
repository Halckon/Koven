//! Form or preserve a callee before evaluating operands and invoke its concrete ABI.
use super::*;
use crate::ssa::refine::body::RTerm;

impl Builder<'_, '_> {
    pub(in crate::ssa::refine::build) fn prepare_callable_callee(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<PreparedCallee>, LoweringError> {
        let Expression::Call { callee, .. } = self.node(expression)?.0 else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let mut root = callee;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        if let (Expression::Name, callee_span) = self.node(root)?
            && let Ok(symbol) = self.reference(callee_span)
        {
            if let Some(&view) = self.borrowed.get(&symbol) {
                return Ok(Some(PreparedCallee::View(view)));
            }
            if let Some(&binding) = self.bindings.get(&symbol) {
                return Ok(Some(PreparedCallee::Value {
                    callable: binding,
                    consumed_callee: None,
                }));
            }
        }
        let Lowered::Value(slot) = self.expression_inner(callee)? else {
            return Ok(None);
        };
        self.register_temporary(callee, slot)?;
        Ok(Some(PreparedCallee::Value {
            callable: slot,
            consumed_callee: Some(callee),
        }))
    }

    /// Calls a previously evaluated view parameter (`CallableInvokeView`) or callable value
    /// (`CallableInvoke`).
    pub(in crate::ssa::refine::build) fn invoke_prepared_callable(
        &mut self,
        expression: ExpressionId,
        prepared: PreparedCallee,
        arguments: Vec<Slot>,
        return_type: UnitTypeId,
        span: Span,
    ) -> Result<(Lowered, Option<ExpressionId>), LoweringError> {
        let (op, consumed_callee) = match prepared {
            PreparedCallee::View(view) => (ROp::CallableInvokeView { view, arguments }, None),
            PreparedCallee::Value {
                callable,
                consumed_callee,
            } => (
                ROp::CallableInvoke {
                    callable,
                    arguments,
                },
                consumed_callee,
            ),
        };
        let result = if self.is_nothing(return_type) {
            self.push(op, Vec::new(), span)?;
            self.terminate(super::RTerm::Unreachable, span)?;
            Lowered::Diverged
        } else if self.is_unit(return_type) {
            self.push(op, Vec::new(), span)?;
            Lowered::Unit
        } else {
            Lowered::Value(self.typed_value(op, expression)?)
        };
        Ok((result, consumed_callee))
    }
}
