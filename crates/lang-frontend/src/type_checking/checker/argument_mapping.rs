//! 单文件 checker 对共享实参映射内核的薄适配。

use crate::{parser::CallArgument, source::Span, type_checking::TypeCheckingError};

use super::Checker;
use crate::type_checking::argument_mapping::map_arguments;
pub(super) use crate::type_checking::argument_mapping::{
    MappedParameter, MappingError, parameter_mode_span,
};

impl Checker<'_> {
    pub(super) fn map_arguments(
        &self,
        parameters: &[MappedParameter<crate::type_checking::TypeId>],
        arguments: &[CallArgument],
        call_span: Span,
    ) -> Result<Result<Vec<usize>, MappingError>, TypeCheckingError> {
        map_arguments(
            self.sources,
            parameters,
            arguments,
            call_span,
            |expression| {
                Ok(self.expression_types[expression.index()].map_or_else(
                    || self.is_syntactic_place(expression),
                    |_| {
                        self.expression_categories[expression.index()]
                            == crate::type_checking::ExpressionCategory::Place
                            && self.is_mutable_element_place(expression).unwrap_or(true)
                    },
                ))
            },
        )
    }

    pub(super) fn emit_mapping_error(
        &mut self,
        error: MappingError,
    ) -> Result<(), TypeCheckingError> {
        match error {
            MappingError::Named(primary) => self.emit(
                self.invalid_named_argument_code,
                "named argument does not map uniquely to a callable parameter",
                primary,
            ),
            MappingError::Arity(primary) => self.emit(
                self.call_argument_arity_code,
                "call must fill every parameter exactly once",
                primary,
            ),
            MappingError::Mode { primary, parameter } => {
                if let Some(parameter) = parameter {
                    self.emit_with_label(
                        self.call_argument_mode_code,
                        "argument marker does not match the parameter contract",
                        primary,
                        parameter,
                        "parameter contract declared here",
                    )
                } else {
                    self.emit(
                        self.call_argument_mode_code,
                        "argument marker does not match the parameter contract",
                        primary,
                    )
                }
            }
        }
    }
}
