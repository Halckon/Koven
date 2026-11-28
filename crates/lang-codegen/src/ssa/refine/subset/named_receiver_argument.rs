//! Exact source/parameter boundary for the bounded owned shared receiver route.
use super::Checker;
use lang_frontend::{
    parser::{CallArgument, Expression, IntegerLiteralKind, LiteralKind},
    type_checking::{BuiltinType, ParameterMode, UnitCallDescriptor},
};

impl Checker<'_, '_> {
    pub(super) fn receiver_arguments_fit(
        &self,
        arguments: &[CallArgument],
        call: &UnitCallDescriptor,
    ) -> Option<bool> {
        Some(match (arguments, call.arguments()) {
            ([], []) => true,
            ([argument], [typed]) => {
                let int = self.inputs.typed.types().builtin(BuiltinType::Int)?;
                let node = self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .ok()?;
                argument.named_prefix.is_none()
                    && argument.mode_marker.is_none()
                    && matches!(
                        node.payload(),
                        Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
                    )
                    && typed.argument_index() == 0
                    && typed.parameter_index() == 0
                    && typed.mode() == ParameterMode::Value
                    && self.c(typed.parameter_type()) == int
                    && self.inputs.typed.expression_type(self.id(argument.value)) == Some(int)
                    && self.c(call.return_type()) == int
            }
            _ => false,
        })
    }
}
