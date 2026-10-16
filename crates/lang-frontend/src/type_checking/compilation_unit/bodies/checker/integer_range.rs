use super::*;
use crate::{
    parser::BinaryOperator,
    type_checking::{UnitIntegerRangeOperationDescriptor, integer_range::select_range_operation},
};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_integer_range_binary(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        operator: BinaryOperator,
        span: Span,
        left_result: ExpressionCheck,
        right_result: ExpressionCheck,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let Some(selected) = select_range_operation(
            operator,
            self.builtin_kind(left_result.ty),
            self.builtin_kind(right_result.ty),
        ) else {
            return Ok(None);
        };
        let ty = if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
            self.error_type()
        } else if let Some((kind, result)) = selected {
            let ty = self.builtin(result);
            self.parts
                .integer_range_operations
                .push(UnitIntegerRangeOperationDescriptor::new(
                    UnitExpressionId::new(source, expression),
                    UnitExpressionId::new(source, left),
                    UnitExpressionId::new(source, right),
                    left_result.ty,
                    right_result.ty,
                    ty,
                    kind,
                ));
            ty
        } else {
            self.emit_binary_operand_error(
                source,
                span,
                left,
                left_result.ty,
                right,
                right_result.ty,
            )?;
            self.error_type()
        };
        Ok(Some(ExpressionCheck {
            ty,
            falls_through: left_result.falls_through && right_result.falls_through,
        }))
    }
}
