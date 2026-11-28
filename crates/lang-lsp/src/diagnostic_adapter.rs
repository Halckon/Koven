//! frontend 结构化诊断到标准 LSP diagnostic 的纯转换。

use std::{collections::HashMap, error::Error, fmt};

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

/// 按 primary source 分组转换 unit diagnostics；related label 使用自身 source URI。
#[cfg(test)]
pub(crate) fn convert_unit_diagnostics(
    sources: &SourceMap,
    uris: &[(SourceId, Uri)],
    diagnostics: &[Diagnostic],
) -> Result<Vec<Vec<lsp_types::Diagnostic>>, DiagnosticMappingError> {
    convert_unit_diagnostics_with_context(sources, uris, diagnostics, false, None)
}

/// 同 `convert_unit_diagnostics`，`enrich_context = true` 时在消息首行附加出错源码片段（显式 opt-in）。
pub(crate) fn convert_unit_diagnostics_with_context(
    sources: &SourceMap,
    uris: &[(SourceId, Uri)],
    diagnostics: &[Diagnostic],
    enrich_context: bool,
    embedded_source: Option<SourceId>,
) -> Result<Vec<Vec<lsp_types::Diagnostic>>, DiagnosticMappingError> {
    let mut grouped = vec![Vec::new(); uris.len()];
    if diagnostics.is_empty() {
        return Ok(grouped);
    }
    // Index identity for lookup only. Publication order still follows the input URIs.
    let mut by_source = HashMap::with_capacity(uris.len());
    for (index, (source, uri)) in uris.iter().enumerate() {
        by_source.entry(*source).or_insert((index, uri));
    }
    for diagnostic in diagnostics {
        let source = diagnostic.primary_span().source_id();
        let (index, _) = by_source
            .get(&source)
            .copied()
            .ok_or(DiagnosticMappingError::UnknownSource(source))?;
        let converted = convert_diagnostic(
            sources,
            diagnostic,
            |source| by_source.get(&source).map(|(_, uri)| *uri),
            enrich_context,
            embedded_source,
        )?;
        grouped[index].push(converted);
    }
    Ok(grouped)
}

fn convert_diagnostic<'uri>(
    sources: &SourceMap,
    diagnostic: &Diagnostic,
    uri_for_source: impl Fn(SourceId) -> Option<&'uri Uri>,
    enrich_context: bool,
    embedded_source: Option<SourceId>,
) -> Result<lsp_types::Diagnostic, DiagnosticMappingError> {
    let mut message = diagnostic.message().to_owned();
    if enrich_context
        && let Some(snippet) = primary_span_snippet(sources, diagnostic.primary_span())
        && !message.contains(snippet)
    {
        message.push_str(" `");
        message.push_str(snippet);
        message.push('`');
    }
    let mut related = Vec::new();
    for detail in diagnostic.details() {
        match detail {
            DiagnosticDetail::Label(label) => {
                let source = label.span().source_id();
                let uri = match uri_for_source(source) {
                    Some(uri) => uri,
                    None if Some(source) == embedded_source => {
                        message.push_str("\nnote: ");
                        message.push_str(label.message());
                        continue;
                    }
                    None => return Err(DiagnosticMappingError::UnknownSource(source)),
                };
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

fn primary_span_snippet(sources: &SourceMap, span: lang_frontend::source::Span) -> Option<&str> {
    let snippet = sources.slice(span).ok()?.trim();
    if snippet.is_empty() || snippet.len() > 60 || snippet.contains(['\n', '\r']) {
        return None;
    }
    Some(snippet)
}

#[cfg(test)]
mod tests {
    use lang_frontend::{
        diagnostic::{Diagnostic, Severity, codes},
        source::SourceMap,
    };
    use lsp_types::{DiagnosticSeverity, NumberOrString, Position, Range, Uri};

    use super::convert_unit_diagnostics;

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

        let mapped = convert_unit_diagnostics(&sources, &[(source, uri.clone())], &[diagnostic])
            .expect("mapping")
            .remove(0);
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
        let mapped_warning =
            convert_unit_diagnostics(&sources, &[(source, uri.clone())], &[warning])
                .expect("mapping")
                .remove(0);
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

        let another = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0080").expect("code"),
            "next diagnostic in the same file",
            primary,
        )
        .expect("diagnostic");
        let provider_diagnostic = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0080").expect("code"),
            "provider diagnostic",
            target,
        )
        .expect("diagnostic");
        let grouped = convert_unit_diagnostics(
            &sources,
            &[
                (consumer, consumer_uri),
                (provider, provider_uri.clone()),
                (provider, "file:///alias.ko".parse().expect("alias URI")),
            ],
            &[diagnostic, provider_diagnostic, another],
        )
        .expect("unit diagnostic mapping");

        assert_eq!(grouped.len(), 3);
        assert_eq!(grouped[1].len(), 1);
        assert_eq!(grouped[1][0].message, "provider diagnostic");
        assert!(grouped[2].is_empty());
        assert_eq!(grouped[0].len(), 2);
        assert_eq!(grouped[0][0].message, "cross-source diagnostic");
        assert_eq!(grouped[0][1].message, "next diagnostic in the same file");
        let related = grouped[0][0]
            .related_information
            .as_ref()
            .expect("related information");
        assert_eq!(related[0].location.uri, provider_uri);
        assert_eq!(related[0].location.range.start, Position::new(0, 4));
    }

    #[test]
    fn rejects_foreign_primary_and_unmapped_related_sources() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("actual", "foo").expect("source");
        let mut other = SourceMap::new();
        let foreign = other.add_source("foreign", "foo").expect("foreign");
        let actual_span = sources.span(source, 0, 3).expect("span");
        let catalog = codes::catalog().expect("catalog");
        let mut diagnostic = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0080").expect("code"),
            "unresolved name",
            actual_span,
        )
        .expect("diagnostic");
        let uri = "file:///foreign.ko".parse().expect("uri");

        assert!(matches!(
            convert_unit_diagnostics(&sources, &[(foreign, uri)], std::slice::from_ref(&diagnostic)),
            Err(super::DiagnosticMappingError::UnknownSource(id)) if id == source
        ));

        let related = sources.add_source("unmapped", "bar").expect("related");
        diagnostic
            .add_label(
                &sources,
                sources.span(related, 0, 3).expect("span"),
                "details",
            )
            .expect("label");
        let uri = "file:///actual.ko".parse().expect("uri");
        assert!(matches!(
            convert_unit_diagnostics(&sources, &[(source, uri)], &[diagnostic]),
            Err(super::DiagnosticMappingError::UnknownSource(id)) if id == related
        ));
    }

    #[test]
    fn enrich_context_appends_primary_span_snippet_only_when_enabled() {
        let mut sources = SourceMap::new();
        let src = sources
            .add_source("file:///demo.ko", "fun main() { scaledDistance() }")
            .expect("source");
        let start = "fun main() { ".len();
        let end = start + "scaledDistance".len();
        let primary = sources.span(src, start, end).expect("span");
        let catalog = codes::catalog().expect("catalog");
        let diag = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0080").expect("code"),
            "unresolved name",
            primary,
        )
        .expect("diag");
        let uri: Uri = "file:///demo.ko".parse().expect("uri");

        let plain = super::convert_unit_diagnostics_with_context(
            &sources,
            &[(src, uri.clone())],
            std::slice::from_ref(&diag),
            false,
            None,
        )
        .expect("plain");
        assert_eq!(plain[0][0].message, "unresolved name");

        let enriched = super::convert_unit_diagnostics_with_context(
            &sources,
            &[(src, uri)],
            &[diag],
            true,
            None,
        )
        .expect("enriched");
        assert_eq!(enriched[0][0].message, "unresolved name `scaledDistance`");
    }
}
