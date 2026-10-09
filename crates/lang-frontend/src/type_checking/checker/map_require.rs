//! 只读确定 Map 槽位查询的类型合同。
use super::super::{Checker, ExprCheck};
use crate::{
    ast::ExpressionId,
    parser::CallArgument,
    source::Span,
    type_checking::{TypeCheckingError, TypeId},
};
impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_require_call(
        &mut self,
        call_expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: TypeId,
        value_type: TypeId,
        arguments: &[CallArgument],
        call_span: Span,
        marker_span: Span,
        source_span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if arguments.len() != 1 {
            self.emit(
                self.call_argument_arity_code,
                "Map.requireValue expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let arg = &arguments[0];
        let arg_result = self.check_expression(arg.value, Some(key_type), None)?;
        if !self.is_error(arg_result.ty)
            && !self.is_deferred(arg_result.ty)
            && !self.assignable(arg_result.ty, key_type)
        {
            self.mismatch(
                self.ast().expressions().get(arg.value)?.span(),
                None,
                arg_result.ty,
                key_type,
            )?;
        }

        self.map_descriptors
            .requires
            .push(crate::type_checking::MapRequireValueDescriptor::new(
                call_expression,
                receiver,
                arg.value,
                key_type,
                value_type,
                marker_span,
                source_span,
            ));
        Ok(ExprCheck {
            ty: value_type,
            falls_through: receiver_falls_through && arg_result.falls_through,
        })
    }
}
