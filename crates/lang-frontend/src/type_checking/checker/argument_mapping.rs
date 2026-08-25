//! Callable 与 construction 共用的确定性实参映射。

use std::collections::BTreeSet;

use crate::{
    parser::{CallArgument, ParameterModeMarker},
    source::Span,
    type_checking::{ExpressionCategory, ParameterMode, TypeCheckingError, TypeId},
};

use super::Checker;

#[derive(Clone)]
pub(super) struct MappedParameter {
    pub(super) name: Option<String>,
    pub(super) mode: ParameterMode,
    pub(super) ty: TypeId,
    pub(super) span: Option<Span>,
}

#[derive(Clone, Copy)]
pub(super) enum MappingError {
    Named(Span),
    Arity(Span),
    Mode {
        primary: Span,
        parameter: Option<Span>,
    },
}

impl Checker<'_> {
    pub(super) fn map_arguments(
        &self,
        parameters: &[MappedParameter],
        arguments: &[CallArgument],
        call_span: Span,
    ) -> Result<Result<Vec<usize>, MappingError>, TypeCheckingError> {
        let mut mapping = Vec::with_capacity(arguments.len());
        let mut used = BTreeSet::new();
        let mut next_position = 0;
        let mut saw_named = false;
        for argument in arguments {
            let parameter_index = if let Some(prefix) = argument.named_prefix {
                saw_named = true;
                let name = self.sources.slice(prefix.name_span)?;
                let Some(index) = parameters
                    .iter()
                    .position(|parameter| parameter.name.as_deref() == Some(name))
                else {
                    return Ok(Err(MappingError::Named(prefix.name_span)));
                };
                if !used.insert(index) {
                    return Ok(Err(MappingError::Named(prefix.name_span)));
                }
                index
            } else {
                if saw_named {
                    return Ok(Err(MappingError::Named(argument.span)));
                }
                while used.contains(&next_position) {
                    next_position += 1;
                }
                if next_position >= parameters.len() {
                    return Ok(Err(MappingError::Arity(argument.span)));
                }
                let index = next_position;
                used.insert(index);
                next_position += 1;
                index
            };
            let parameter = &parameters[parameter_index];
            let mode_matches = match (argument.mode_marker, parameter.mode) {
                (None, ParameterMode::Value | ParameterMode::Borrow) => true,
                (Some(ParameterModeMarker::Borrow(_)), ParameterMode::Borrow) => true,
                (Some(ParameterModeMarker::Inout(_)), ParameterMode::Inout) => {
                    self.expression_types[argument.value.index()].map_or_else(
                        || self.is_syntactic_place(argument.value),
                        |_| {
                            self.expression_categories[argument.value.index()]
                                == ExpressionCategory::Place
                                && self
                                    .is_mutable_element_place(argument.value)
                                    .unwrap_or(true)
                        },
                    )
                }
                _ => false,
            };
            if !mode_matches {
                return Ok(Err(MappingError::Mode {
                    primary: argument
                        .mode_marker
                        .map(parameter_mode_span)
                        .unwrap_or(argument.span),
                    parameter: parameter.span,
                }));
            }
            mapping.push(parameter_index);
        }
        if used.len() != parameters.len() {
            return Ok(Err(MappingError::Arity(call_span)));
        }
        Ok(Ok(mapping))
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

pub(super) const fn parameter_mode_span(marker: ParameterModeMarker) -> Span {
    match marker {
        ParameterModeMarker::Own(span)
        | ParameterModeMarker::Borrow(span)
        | ParameterModeMarker::Inout(span) => span,
    }
}
