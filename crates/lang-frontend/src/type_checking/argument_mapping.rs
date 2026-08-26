//! 单文件与 compilation-unit checker 共用的确定性实参映射内核。

use std::collections::BTreeSet;

use crate::{
    ast::ExpressionId,
    parser::{CallArgument, ParameterModeMarker},
    source::{SourceMap, Span},
};

use super::{ParameterMode, TypeCheckingError};

#[derive(Clone)]
pub(in crate::type_checking) struct MappedParameter<T> {
    pub(in crate::type_checking) name: Option<String>,
    pub(in crate::type_checking) mode: ParameterMode,
    pub(in crate::type_checking) ty: T,
    pub(in crate::type_checking) span: Option<Span>,
}

#[derive(Clone, Copy)]
pub(in crate::type_checking) enum MappingError {
    Named(Span),
    Arity(Span),
    Mode {
        primary: Span,
        parameter: Option<Span>,
    },
}

pub(in crate::type_checking) fn map_arguments<T>(
    sources: &SourceMap,
    parameters: &[MappedParameter<T>],
    arguments: &[CallArgument],
    call_span: Span,
    mut is_inout_place: impl FnMut(ExpressionId) -> Result<bool, TypeCheckingError>,
) -> Result<Result<Vec<usize>, MappingError>, TypeCheckingError> {
    let mut mapping = Vec::with_capacity(arguments.len());
    let mut used = BTreeSet::new();
    let mut next_position = 0;
    let mut saw_named = false;
    for argument in arguments {
        let parameter_index = if let Some(prefix) = argument.named_prefix {
            saw_named = true;
            let name = sources.slice(prefix.name_span)?;
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
                is_inout_place(argument.value)?
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

pub(in crate::type_checking) const fn parameter_mode_span(marker: ParameterModeMarker) -> Span {
    match marker {
        ParameterModeMarker::Own(span)
        | ParameterModeMarker::Borrow(span)
        | ParameterModeMarker::Inout(span) => span,
    }
}
