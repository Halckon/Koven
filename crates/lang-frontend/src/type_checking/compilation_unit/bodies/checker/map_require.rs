//! source-qualified 确定 Map 槽位查询。
use super::super::{BodyChecker, ExpressionCheck};
use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::CallArgument,
    source::Span,
    type_checking::{CompilationUnitTypeError, TypeCheckingError, UnitTypeId},
};
impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_require_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        arguments: &[CallArgument],
        call_span: Span,
        marker_span: Span,
        source_span: Span,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if arguments.len() != 1 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "Map.requireValue expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let arg = &arguments[0];
        let arg_result =
            self.check_expression(source, arg.value, Some(key_type), None, return_type)?;
        if !self.is_error(arg_result.ty)
            && !self.is_deferred(arg_result.ty)
            && !self.assignable(arg_result.ty, key_type)
        {
            self.mismatch(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(arg.value)
                    .map_err(TypeCheckingError::from)?
                    .span(),
                None,
                arg_result.ty,
                key_type,
            )?;
        }

        self.parts.map_descriptors.requires.push(
            super::super::super::map::UnitMapRequireValueDescriptor::new(
                crate::type_checking::UnitExpressionId::new(source, expression),
                crate::type_checking::UnitExpressionId::new(source, receiver),
                crate::type_checking::UnitExpressionId::new(source, arg.value),
                key_type,
                value_type,
                marker_span,
                source_span,
            ),
        );
        Ok(ExpressionCheck {
            ty: value_type,
            falls_through: receiver_falls_through && arg_result.falls_through,
        })
    }
}
