//! frontend 结构化诊断到标准 LSP diagnostic 的纯转换。

use std::{error::Error, fmt};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    source::{SourceId, SourceMap},
};
use lsp_types::{DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString, Uri};

use crate::position_adapter::{PositionMappingError, span_range};

/// diagnostic 的 source/UTF-16 映射失败。
#[derive(Debug)]
pub(crate) enum DiagnosticMappingError {
    Position(PositionMappingError),
    UnknownSource(SourceId),
}

impl fmt::Display for DiagnosticMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Position(error) => {
                write!(formatter, "diagnostic position mapping failed: {error}")
            }
            Self::UnknownSource(source) => {
                write!(
                    formatter,
                    "diagnostic references unmapped source {source:?}"
                )
            }
        }
    }
}

impl Error for DiagnosticMappingError {}

impl From<PositionMappingError> for DiagnosticMappingError {
    fn from(error: PositionMappingError) -> Self {
        Self::Position(error)
    }
}

/// 把确定性 frontend 诊断转换为同序的 LSP 诊断。
pub(crate) fn convert_diagnostics(
    sources: &SourceMap,
    uri: &Uri,
    diagnostics: &[Diagnostic],
) -> Result<Vec<lsp_types::Diagnostic>, DiagnosticMappingError> {
    diagnostics
        .iter()
        .map(|diagnostic| convert_diagnostic(sources, diagnostic, |_| Some(uri)))
        .collect()
}

/// 按 primary source 分组转换 unit diagnostics；related label 使用自身 source URI。
pub(crate) fn convert_unit_diagnostics(
    sources: &SourceMap,
    uris: &[(SourceId, Uri)],
    diagnostics: &[Diagnostic],
) -> Result<Vec<Vec<lsp_types::Diagnostic>>, DiagnosticMappingError> {
    let mut grouped = vec![Vec::new(); uris.len()];
    for diagnostic in diagnostics {
        let source = diagnostic.primary_span().source_id();
        let index = uris
            .iter()
            .position(|(candidate, _)| *candidate == source)
            .ok_or(DiagnosticMappingError::UnknownSource(source))?;
        let converted = convert_diagnostic(sources, diagnostic, |source| {
            uris.iter()
                .find(|(candidate, _)| *candidate == source)
                .map(|(_, uri)| uri)
        })?;
        grouped[index].push(converted);
    }
    Ok(grouped)
}

fn convert_diagnostic<'uri>(
    sources: &SourceMap,
    diagnostic: &Diagnostic,
    uri_for_source: impl Fn(SourceId) -> Option<&'uri Uri>,
) -> Result<lsp_types::Diagnostic, DiagnosticMappingError> {
    let _primary_uri = uri_for_source(diagnostic.primary_span().source_id()).ok_or(
        DiagnosticMappingError::UnknownSource(diagnostic.primary_span().source_id()),
    )?;
    let mut message = diagnostic.message().to_owned();
    let mut related = Vec::new();
    for detail in diagnostic.details() {
        match detail {
            DiagnosticDetail::Label(label) => {
                let uri = uri_for_source(label.span().source_id()).ok_or(
                    DiagnosticMappingError::UnknownSource(label.span().source_id()),
                )?;
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

#[cfg(test)]
mod tests {
    use lang_frontend::{
        diagnostic::{Diagnostic, Severity, codes},
        source::SourceMap,
    };
    use lsp_types::{DiagnosticSeverity, NumberOrString, Position, Range, Uri};

    use super::{convert_diagnostics, convert_unit_diagnostics};

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

    #[test]
    fn maps_unit_primary_and_related_locations_to_their_own_uris() {
        let mut sources = SourceMap::new();
        let provider = sources
            .add_source("provider", "fun target(): Unit {}")
            .expect("provider");
        let consumer = sources
            .add_source("consumer", "fun use(): Unit { target() }")
            .expect("consumer");
        let primary = sources.span(consumer, 18, 24).expect("primary");
        let target = sources.span(provider, 4, 10).expect("target");
        let catalog = codes::catalog().expect("catalog");
        let mut diagnostic = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0080").expect("code"),
            "cross-source diagnostic",
            primary,
        )
        .expect("diagnostic");
        diagnostic
            .add_label(&sources, target, "declared here")
            .expect("related label");
        let provider_uri: Uri = "file:///workspace/p/provider.ko".parse().expect("uri");
        let consumer_uri: Uri = "file:///workspace/q/consumer.ko".parse().expect("uri");

        let grouped = convert_unit_diagnostics(
            &sources,
            &[(provider, provider_uri.clone()), (consumer, consumer_uri)],
            &[diagnostic],
        )
        .expect("unit diagnostic mapping");

        assert!(grouped[0].is_empty());
        assert_eq!(grouped[1].len(), 1);
        let related = grouped[1][0]
            .related_information
            .as_ref()
            .expect("related information");
        assert_eq!(related[0].location.uri, provider_uri);
        assert_eq!(related[0].location.range.start, Position::new(0, 4));
    }
}
