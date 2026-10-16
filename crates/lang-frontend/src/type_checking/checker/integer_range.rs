use super::*;
use crate::{parser::BinaryOperator, type_checking::integer_range::select_range_operation};

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_integer_range_binary(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        operator: BinaryOperator,
        span: Span,
        left_result: ExprCheck,
        right_result: ExprCheck,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let builtin = |ty| match self.kind(ty) {
            TypeKind::Builtin(builtin) => Some(*builtin),
            _ => None,
        };
        let Some(selected) =
            select_range_operation(operator, builtin(left_result.ty), builtin(right_result.ty))
        else {
            return Ok(None);
        };
        let ty = if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
            self.error_type()
        } else if let Some((kind, result)) = selected {
            let ty = self.builtin(result);
            self.integer_range_operations
                .push(IntegerRangeOperationDescriptor::new(
                    expression,
                    left,
                    right,
                    left_result.ty,
                    right_result.ty,
                    ty,
                    kind,
                ));
            ty
        } else {
            self.emit_binary_operand_error(span, left, left_result.ty, right, right_result.ty)?;
            self.error_type()
        };
        Ok(Some(ExprCheck {
            ty,
            falls_through: left_result.falls_through && right_result.falls_through,
        }))
    }
}
