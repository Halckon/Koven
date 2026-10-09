//! 同步作用域 Map 访问的预期 Borrow V callback 类型。
use super::super::{Checker, ExprCheck};
use crate::{
    ast::ExpressionId,
    parser::CallArgument,
    source::Span,
    type_checking::{
        BuiltinType, FunctionParameterType, MapWithValueDescriptor, ParameterMode,
        TypeCheckingError, TypeId, TypeKind,
    },
};
impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_with_call(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: TypeId,
        value_type: TypeId,
        arguments: &[CallArgument],
        span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if arguments.len() != 2 {
            self.emit(
                self.call_argument_arity_code,
                "Map.withValue expects exactly 2 arguments",
                span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }
        let unit = self.builtin(BuiltinType::Unit);
        let action_type = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: vec![FunctionParameterType {
                mode: ParameterMode::Borrow,
                ty: value_type,
            }],
            return_type: unit,
        });
        let mut falls_through = receiver_falls_through;
        for (argument, expected) in arguments.iter().zip([key_type, action_type]) {
            let result = self.check_expression(argument.value, Some(expected), None)?;
            falls_through &= result.falls_through;
            if !self.is_error(result.ty)
                && !self.is_deferred(result.ty)
                && !self.assignable(result.ty, expected)
            {
                self.mismatch(
                    self.ast().expressions().get(argument.value)?.span(),
                    None,
                    result.ty,
                    expected,
                )?;
            }
        }
        self.map_descriptors
            .with_values
            .push(MapWithValueDescriptor::new(
                expression,
                receiver,
                arguments[0].value,
                arguments[1].value,
                key_type,
                value_type,
                action_type,
            ));
        Ok(ExprCheck {
            ty: self.builtin(BuiltinType::Boolean),
            falls_through,
        })
    }
}
