//! ADR-0014 版本化 JSON Lines 机器诊断适配边界。

use std::{error::Error, fmt};

use lang_frontend::{
    diagnostic::{
        Diagnostic, DiagnosticDetail, DiagnosticError, Severity, SpanRole, ordered_diagnostics,
    },
    source::{SourceMap, Span},
};
use serde_json::{Value, json};

/// 把完整诊断集合编码为 ADR-0014 schema v1 JSON Lines。
///
/// 本函数在返回前解析并编码全部记录；任一范围或 JSON 错误都不会产生部分输出。
pub(super) fn render_machine_diagnostics(
    sources: &SourceMap,
    diagnostics: &[Diagnostic],
) -> Result<String, MachineDiagnosticError> {
    let ordered = ordered_diagnostics(sources, diagnostics)?;
    let records = ordered
        .into_iter()
        .map(|diagnostic| diagnostic_value(sources, diagnostic))
        .collect::<Result<Vec<_>, _>>()?;
    let mut rendered = String::new();
    for record in records {
        rendered.push_str(&serde_json::to_string(&record)?);
        rendered.push('\n');
    }
    Ok(rendered)
}

fn diagnostic_value(
    sources: &SourceMap,
    diagnostic: &Diagnostic,
) -> Result<Value, MachineDiagnosticError> {
    let details = diagnostic
        .details()
        .iter()
        .enumerate()
        .map(|(detail_index, detail)| match detail {
            DiagnosticDetail::Label(label) => Ok(json!({
                "kind": "label",
                "message": label.message(),
                "location": location_value(
                    sources,
                    SpanRole::Label { detail_index },
                    label.span(),
                )?,
            })),
            DiagnosticDetail::Note(text) => Ok(json!({
                "kind": "note",
                "message": text.as_str(),
            })),
            DiagnosticDetail::Help(text) => Ok(json!({
                "kind": "help",
                "message": text.as_str(),
            })),
        })
        .collect::<Result<Vec<_>, MachineDiagnosticError>>()?;

    Ok(json!({
        "schema": "koven.diagnostic",
        "version": 1,
        "severity": severity_name(diagnostic.severity()),
        "code": diagnostic.code().to_string(),
        "message": diagnostic.message(),
        "primary": location_value(sources, SpanRole::Primary, diagnostic.primary_span())?,
        "details": details,
    }))
}

fn location_value(
    sources: &SourceMap,
    role: SpanRole,
    span: Span,
) -> Result<Value, MachineDiagnosticError> {
    let source = sources
        .source_name(span.source_id())
        .map_err(|source| DiagnosticError::InvalidSpan { role, source })?;
    let start = sources
        .position(span.source_id(), span.start())
        .map_err(|source| DiagnosticError::InvalidSpan { role, source })?;
    let end = sources
        .position(span.source_id(), span.end())
        .map_err(|source| DiagnosticError::InvalidSpan { role, source })?;

    Ok(json!({
        "source": source,
        "byte_start": span.start(),
        "byte_end": span.end(),
        "start": { "line": start.line(), "column": start.column() },
        "end": { "line": end.line(), "column": end.column() },
    }))
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

#[derive(Debug)]
pub(super) enum MachineDiagnosticError {
    Diagnostic(DiagnosticError),
    Json(serde_json::Error),
}

impl fmt::Display for MachineDiagnosticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostic(error) => write!(formatter, "invalid diagnostic: {error}"),
            Self::Json(error) => write!(formatter, "JSON encoding failed: {error}"),
        }
    }
}

impl Error for MachineDiagnosticError {}

impl From<DiagnosticError> for MachineDiagnosticError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}

impl From<serde_json::Error> for MachineDiagnosticError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests;
