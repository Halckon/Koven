//! 前端事实证明之后的后端能力门；不改变任何语义错误的事实撤销条件。
use super::OwnershipCheckingError;
use crate::{
    diagnostic::{Diagnostic, Severity, codes},
    source::{SourceMap, Span},
    type_checking::{CompilationUnitTypes, TypedFile},
};
fn diagnostics(
    sources: &SourceMap,
    spans: impl Iterator<Item = Span>,
) -> Result<Vec<Diagnostic>, OwnershipCheckingError> {
    spans.map(|span| Diagnostic::new(sources, Severity::Error,
        codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
        "trusted range receiver ownership is checked; receiver SSA delivery is not yet implemented", span).map_err(OwnershipCheckingError::from)).collect()
}
pub(super) fn single(
    sources: &SourceMap,
    typed: &TypedFile,
) -> Result<Vec<Diagnostic>, OwnershipCheckingError> {
    diagnostics(
        sources,
        typed
            .callables()
            .iter()
            .filter_map(|c| c.range_extension().map(|b| b.receiver_span())),
    )
}
pub(super) fn unit(
    sources: &SourceMap,
    typed: &CompilationUnitTypes,
) -> Result<Vec<Diagnostic>, OwnershipCheckingError> {
    diagnostics(
        sources,
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter_map(|c| c.range_extension().map(|b| b.receiver_span())),
    )
}
