//! Shared expression algorithms; entry adapters retain identity and stateful submission.
pub(in crate::ssa) mod checked;

use lang_frontend::source::Span;

use super::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{ScalarConstant, ValueId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
}

pub(in crate::ssa) fn parse_float_literal(
    sources: &lang_frontend::source::SourceMap,
    kind: lang_frontend::parser::FloatLiteralKind,
    span: Span,
) -> Result<ScalarConstant, LoweringError> {
    use lang_frontend::parser::FloatLiteralKind;
    let text = sources
        .slice(span)
        .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
    let invalid = || error(LoweringErrorKind::InvalidLiteral, span);
    let digits =
        lang_frontend::type_checking::float_literal_core(text, kind).ok_or_else(invalid)?;
    match kind {
        FloatLiteralKind::Float => digits
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|value| ScalarConstant::Float32(value.to_bits())),
        FloatLiteralKind::Double => digits
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|value| ScalarConstant::Float64(value.to_bits())),
    }
    .ok_or_else(invalid)
}
