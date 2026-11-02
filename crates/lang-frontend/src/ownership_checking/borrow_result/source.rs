//! Shared syntax selection; symbol identity remains local to each checker.
use super::{ReturnSource, marker_span};
use crate::{
    parser::{BorrowReturnSource, FunctionForm, NameMarker, ParameterModeMarker, ValueParameter},
    source::SourceMap,
};

pub(crate) fn borrow_return_source<S>(
    sources: &SourceMap,
    form: FunctionForm,
    parameters: &[ValueParameter],
    receiver: Option<S>,
    mut resolve: impl FnMut(NameMarker) -> Option<S>,
) -> Option<ReturnSource<S>> {
    let FunctionForm::Explicit {
        result_source: Some(syntax),
        ..
    } = form
    else {
        return None;
    };
    let (declared_source, marker, new_range) = match syntax {
        crate::parser::FunctionResultSource::Borrow(syntax) => {
            (syntax.source, syntax.borrow_span, false)
        }
        crate::parser::FunctionResultSource::Carrier(syntax) => {
            (syntax.source, syntax.from_span, true)
        }
    };
    let (symbol, span) = match declared_source {
        BorrowReturnSource::Parameter(NameMarker::Present(span)) => {
            let parameter = parameters.iter().find(|parameter| {
                sources.slice(marker_span(parameter.name)).ok() == sources.slice(span).ok()
            });
            let symbol = parameter
                .filter(|p| {
                    !matches!(
                        p.mode_marker,
                        Some(ParameterModeMarker::Inout(_) | ParameterModeMarker::Own(_))
                    )
                })
                .and_then(|p| resolve(p.name));
            (symbol, parameter.map_or(span, |p| marker_span(p.name)))
        }
        BorrowReturnSource::Receiver(span) => (receiver, span),
        BorrowReturnSource::Parameter(NameMarker::Missing(span) | NameMarker::Error(span)) => {
            (None, span)
        }
    };
    Some(ReturnSource {
        symbol,
        span,
        marker,
        new_range,
    })
}

#[cfg(test)]
mod tests;
