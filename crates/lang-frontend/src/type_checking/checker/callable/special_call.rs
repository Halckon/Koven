//! 在泛型候选选择前分派构造与 compiler-bound 内建调用，保持原有顺序。
use super::*;

impl Checker<'_> {
    pub(super) fn check_special_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<TypeId>,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        if let Some(result) = self.check_range_construction_call(
            expression,
            call_span,
            callee,
            type_arguments,
            arguments,
        )? {
            return Ok(Some(result));
        }
        if let Some(result) = self.check_construction_call(
            expression,
            call_span,
            callee,
            type_arguments,
            arguments,
            expected,
        )? {
            return Ok(Some(result));
        }
        if let Some(result) =
            self.check_integer_inv_call(expression, call_span, callee, type_arguments, arguments)?
        {
            return Ok(Some(result));
        }
        if let Some(result) =
            self.check_string_clone_call(expression, call_span, callee, type_arguments, arguments)?
        {
            return Ok(Some(result));
        }
        if let Some(result) =
            self.check_rc_share_call(expression, call_span, callee, type_arguments, arguments)?
        {
            return Ok(Some(result));
        }
        if let Some(result) = self.check_intrinsic_ownership_primitive_call(
            expression,
            call_span,
            callee,
            type_arguments,
            arguments,
        )? {
            return Ok(Some(result));
        }
        if let Some(result) = self.check_intrinsic_container_call(
            expression,
            call_span,
            callee,
            type_arguments,
            arguments,
            expected,
        )? {
            return Ok(Some(result));
        }
        Ok(None)
    }
}
