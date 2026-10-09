//! 同步作用域 Map 访问的预期 Borrow V callback 类型。
use super::super::{BodyChecker, ExpressionCheck};
use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::CallArgument,
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, MapWithValueDescriptor, ParameterMode,
        TypeCheckingError, UnitExpressionId, UnitFunctionParameterType, UnitTypeId, UnitTypeKind,
    },
};
impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_with_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        arguments: &[CallArgument],
        span: Span,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if arguments.len() != 2 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "Map.withValue expects exactly 2 arguments",
                span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }
        let unit = self.builtin(BuiltinType::Unit);
        let action_type = self.signatures.types_mut().intern(UnitTypeKind::Function {
            move_only: false,
            parameters: vec![UnitFunctionParameterType::new(
                ParameterMode::Borrow,
                value_type,
            )],
            return_type: unit,
        });
        let mut falls_through = receiver_falls_through;
        for (argument, expected) in arguments.iter().zip([key_type, action_type]) {
            let result =
                self.check_expression(source, argument.value, Some(expected), None, return_type)?;
            falls_through &= result.falls_through;
            if !self.is_error(result.ty)
                && !self.is_deferred(result.ty)
                && !self.assignable(result.ty, expected)
            {
                self.mismatch(
                    self.file(source)
                        .ast()
                        .expressions()
                        .get(argument.value)
                        .map_err(TypeCheckingError::from)?
                        .span(),
                    None,
                    result.ty,
                    expected,
                )?;
            }
        }
        self.parts
            .map_descriptors
            .with_values
            .push(MapWithValueDescriptor::new(
                UnitExpressionId::new(source, expression),
                UnitExpressionId::new(source, receiver),
                UnitExpressionId::new(source, arguments[0].value),
                UnitExpressionId::new(source, arguments[1].value),
                key_type,
                value_type,
                action_type,
            ));
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Boolean),
            falls_through,
        })
    }
}
