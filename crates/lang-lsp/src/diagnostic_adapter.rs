//! frontend 结构化诊断到标准 LSP diagnostic 的纯转换。

use std::{error::Error, fmt};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    source::{SourceError, SourceId, SourceMap, Span},
};
use lsp_types::{
    DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString, Position, Range,
    Uri,
};

/// 把确定性 frontend 诊断转换为同序的 LSP 诊断。
pub(crate) fn convert_diagnostics(
    sources: &SourceMap,
    uri: &Uri,
    diagnostics: &[Diagnostic],
) -> Result<Vec<lsp_types::Diagnostic>, DiagnosticMappingError> {
    diagnostics
        .iter()
        .map(|diagnostic| convert_diagnostic(sources, uri, diagnostic))
        .collect()
}

fn convert_diagnostic(
    sources: &SourceMap,
    uri: &Uri,
    diagnostic: &Diagnostic,
) -> Result<lsp_types::Diagnostic, DiagnosticMappingError> {
    let mut message = diagnostic.message().to_owned();
    let mut related = Vec::new();
    for detail in diagnostic.details() {
        match detail {
            DiagnosticDetail::Label(label) => {
                related.push(DiagnosticRelatedInformation {
                    location: Location::new(uri.clone(), span_range(sources, label.span())?),
                    message: label.message().to_owned(),
                });
            }
            DiagnosticDetail::Note(note) => {
                message.push_str("\nnote: ");
                message.push_str(note.as_str());
            }
            DiagnosticDetail::Help(help) => {
                message.push_str("\nhelp: ");
                message.push_str(help.as_str());
            }
        }
    }

    Ok(lsp_types::Diagnostic::new(
        span_range(sources, diagnostic.primary_span())?,
        Some(match diagnostic.severity() {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        Some(NumberOrString::String(diagnostic.code().to_string())),
        Some("kovenc".to_owned()),
        message,
        (!related.is_empty()).then_some(related),
        None,
    ))
}

fn span_range(sources: &SourceMap, span: Span) -> Result<Range, DiagnosticMappingError> {
    Ok(Range::new(
        utf16_position(sources, span.source_id(), span.start())?,
        utf16_position(sources, span.source_id(), span.end())?,
    ))
}

fn utf16_position(
    sources: &SourceMap,
    source_id: SourceId,
    offset: usize,
) -> Result<Position, DiagnosticMappingError> {
    let source_position = sources.position(source_id, offset)?;
    let text = sources.source_text(source_id)?;
    let scalar_column = source_position.column() - 1;
    // SourceMap owns line/CRLF semantics. Walking exactly its reported scalar column only adapts
    // that column to the UTF-16 code units required by the LSP boundary.
    let utf16_column = text[..offset]
        .chars()
        .rev()
        .take(scalar_column)
        .map(char::len_utf16)
        .sum::<usize>();

    Ok(Position::new(
        u32::try_from(source_position.line() - 1)
            .map_err(|_| DiagnosticMappingError::PositionOverflow)?,
        u32::try_from(utf16_column).map_err(|_| DiagnosticMappingError::PositionOverflow)?,
    ))
}

/// frontend span 无法表示为 LSP range。
#[derive(Debug)]
pub(crate) enum DiagnosticMappingError {
    Source(SourceError),
    PositionOverflow,
}

impl fmt::Display for DiagnosticMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "invalid diagnostic source span: {error}"),
            Self::PositionOverflow => {
                formatter.write_str("source position exceeds the LSP u32 range")
            }
        }
    }
}

impl Error for DiagnosticMappingError {}

impl From<SourceError> for DiagnosticMappingError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

#[cfg(test)]
mod tests {
    use lang_frontend::{
        diagnostic::{Diagnostic, Severity, codes},
        source::SourceMap,
    };
    use lsp_types::{DiagnosticSeverity, NumberOrString, Position, Range, Uri};

    use super::convert_diagnostics;

    #[test]
    fn maps_utf16_crlf_empty_ranges_and_details_without_losing_order() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("file:///unicode.ko", "a😀b\r\nnext")
            .expect("source");
        let primary = sources.span(source, 5, 6).expect("primary span");
        let label_span = sources
            .span(source, "a😀b\r\nnext".len(), "a😀b\r\nnext".len())
            .expect("empty EOF label span");
        let catalog = codes::catalog().expect("catalog");
        let mut diagnostic = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0001").expect("code"),
            "invalid input",
            primary,
        )
        .expect("diagnostic");
        diagnostic
            .add_label(&sources, label_span, "related")
            .expect("label");
        diagnostic.add_note("context").expect("note");
        diagnostic.add_help("replace it").expect("help");
        let uri: Uri = "file:///unicode.ko".parse().expect("uri");

        let mapped = convert_diagnostics(&sources, &uri, &[diagnostic]).expect("mapping");
        let actual = &mapped[0];
        assert_eq!(
            actual.range,
            Range::new(Position::new(0, 3), Position::new(0, 4))
        );
        assert_eq!(actual.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            actual.code,
            Some(NumberOrString::String("L0001".to_owned()))
        );
        assert_eq!(
            actual.message,
            "invalid input\nnote: context\nhelp: replace it"
        );
        let related = actual.related_information.as_ref().expect("related info");
        assert_eq!(related.len(), 1);
        assert_eq!(
            related[0].location.range,
            Range::new(Position::new(1, 4), Position::new(1, 4))
        );
        assert_eq!(related[0].message, "related");

        let warning = Diagnostic::new(
            &sources,
            Severity::Warning,
            catalog.resolve("L0002").expect("warning code"),
            "warning",
            primary,
        )
        .expect("warning diagnostic");
        let mapped_warning = convert_diagnostics(&sources, &uri, &[warning]).expect("mapping");
        assert_eq!(
            mapped_warning[0].severity,
            Some(DiagnosticSeverity::WARNING)
        );
    }
}
