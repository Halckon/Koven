//! 单文件与 compilation-unit adapter 共用的中立 lowering 支撑。

pub(in crate::ssa) mod callable_instances;
pub(in crate::ssa) mod string_literal;

use lang_frontend::{
    parser::{IntegerLiteralKind, NameMarker},
    source::Span,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoweringErrorKind {
    MismatchedSource,
    MismatchedAnalysis,
    FrontendDiagnostics,
    BlockingDeferred,
    InstanceLimitExceeded,
    UnsupportedNode,
    UnsupportedCompanionCall,
    MissingFact,
    InvalidLiteral,
    InvalidModel,
    InvalidSsa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoweringError {
    pub(crate) kind: LoweringErrorKind,
    pub(crate) span: Option<Span>,
}

pub(in crate::ssa) const fn error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}

pub(in crate::ssa) const fn present_name(marker: NameMarker) -> Option<Span> {
    match marker {
        NameMarker::Present(span) => Some(span),
        NameMarker::Missing(_) | NameMarker::Error(_) => None,
    }
}

pub(in crate::ssa) const fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

pub(in crate::ssa) fn parse_integer_literal_text(
    text: &str,
    kind: IntegerLiteralKind,
    span: Span,
) -> Result<i128, LoweringError> {
    lang_frontend::type_checking::integer_literal_magnitude(text, kind)
        .and_then(|value| i128::try_from(value).ok())
        .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))
}
