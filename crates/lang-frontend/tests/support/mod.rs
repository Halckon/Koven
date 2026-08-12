use lang_frontend::diagnostic::{DiagnosticCode, DiagnosticCodeCatalog, DiagnosticCodeError};

pub(crate) fn catalog(codes: &[&str]) -> DiagnosticCodeCatalog {
    DiagnosticCodeCatalog::try_new(codes).expect("test diagnostic codes must be valid and unique")
}

pub(crate) fn code(
    catalog: &DiagnosticCodeCatalog,
    raw: &str,
) -> Result<DiagnosticCode, DiagnosticCodeError> {
    catalog.resolve(raw)
}
