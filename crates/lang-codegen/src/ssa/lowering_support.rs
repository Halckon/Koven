//! 单文件与 compilation-unit adapter 共用的中立 lowering 支撑。

pub(in crate::ssa) mod callable_instances;
pub(in crate::ssa) mod string_literal;

use lang_frontend::source::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoweringErrorKind {
    MismatchedSource,
    MismatchedAnalysis,
    FrontendDiagnostics,
    BlockingDeferred,
    InstanceLimitExceeded,
    UnsupportedNode,
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
