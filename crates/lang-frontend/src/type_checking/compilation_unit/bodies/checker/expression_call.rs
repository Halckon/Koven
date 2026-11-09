//! Keep call dispatch temporaries off recursively nested expression frames.
use super::{BodyChecker, ExpressionCheck, ExpressionExpected, UnitExpressionId};
use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::SourceUnitId,
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{CompilationUnitTypeError, TypeCheckingError, UnitTypeId},
};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_call_expression(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expectation: Option<ExpressionExpected<'_>>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let invalidated = arguments
            .iter()
            .filter(|argument| {
                matches!(
                    argument.mode_marker,
                    Some(crate::parser::ParameterModeMarker::Inout(_))
                )
            })
            .filter_map(|argument| self.stable_flow_key(source, argument.value))
            .collect::<Vec<_>>();
        let call = UnitExpressionId::new(source, expression);
        self.inactive_inout_calls.remove(&call);
        let result = if let Some(result) = self.check_intrinsic_ownership_primitive_call(
            source,
            expression,
            span,
            callee,
            type_arguments,
            arguments,
            return_type,
        )? {
            result
        } else if let Some(result) = self.check_intrinsic_container_call(
            source,
            expression,
            span,
            callee,
            type_arguments,
            arguments,
            expectation,
            expected_span,
            return_type,
        )? {
            result
        } else if let Some(result) = self.check_source_construction_call(
            source,
            expression,
            span,
            callee,
            type_arguments,
            arguments,
            expectation,
            expected_span,
            return_type,
        )? {
            result
        } else {
            let result = self.check_call(
                source,
                expression,
                callee,
                type_arguments,
                arguments,
                return_type,
            )?;
            self.record_pending_call_prefix(
                UnitExpressionId::new(source, expression),
                callee,
                arguments,
            )?;
            result
        };
        if self.is_deferred(result.ty) && self.has_inactive_callee(source, callee)? {
            self.inactive_inout_calls.insert(call);
        }
        if !invalidated.is_empty() {
            if result.falls_through
                && !self.is_error(result.ty)
                && !self.inactive_inout_calls.contains(&call)
            {
                for key in invalidated {
                    self.invalidate_flow_key(key);
                }
            } else {
                // Boolean edge reconstruction must omit only this call's own write.
                self.inactive_inout_calls.insert(call);
            }
        }
        Ok(result)
    }
    fn has_inactive_callee(
        &self,
        source: SourceUnitId,
        callee: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        let mut operands = vec![callee];
        while let Some(operand) = operands.pop() {
            if self
                .inactive_inout_calls
                .contains(&UnitExpressionId::new(source, operand))
            {
                return Ok(true);
            }
            match self
                .file(source)
                .ast()
                .expressions()
                .get(operand)
                .map_err(TypeCheckingError::from)?
                .payload()
            {
                Expression::Group { expression }
                | Expression::Member {
                    receiver: expression,
                    ..
                } => operands.push(*expression),
                Expression::Index { receiver, index } => operands.extend([*receiver, *index]),
                _ => {}
            }
        }
        Ok(false)
    }
}
