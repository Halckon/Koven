use std::{error::Error, fmt};

use crate::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    source::{SourceError, SourceId, SourceMap, Span},
};

use super::{SourceUnit, SourceUnitKey};

/// unit 诊断包含无效 source/span 或 unit 外 source。
#[derive(Debug)]
pub enum UnitDiagnosticOrderError {
    /// 某个诊断范围不属于给定 SourceMap。
    Source(SourceError),
    /// 某个诊断范围属于 SourceMap，但不属于当前 unit。
    SourceOutsideUnit(SourceId),
}

impl fmt::Display for UnitDiagnosticOrderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "invalid diagnostic source/span: {error}"),
            Self::SourceOutsideUnit(source) => {
                write!(
                    formatter,
                    "diagnostic source {source:?} is outside the compilation unit"
                )
            }
        }
    }
}

impl Error for UnitDiagnosticOrderError {}
impl From<SourceError> for UnitDiagnosticOrderError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct OrderKey {
    source: SourceUnitKey,
    start: usize,
    end: usize,
    severity: u8,
    code: String,
    message: String,
    details: Vec<DetailKey>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum DetailKey {
    Label {
        source: SourceUnitKey,
        start: usize,
        end: usize,
        message: String,
    },
    Note(String),
    Help(String),
}

/// 按稳定 source key、范围及全部可渲染字段返回 unit 诊断全序。
///
/// # Errors
///
/// 任一诊断范围无法由 SourceMap 解析，或其 source 不属于给定 unit 时返回具体错误。
pub fn ordered_unit_diagnostics<'diagnostic>(
    sources: &SourceMap,
    source_units: &[SourceUnit],
    diagnostics: &'diagnostic [Diagnostic],
) -> Result<Vec<&'diagnostic Diagnostic>, UnitDiagnosticOrderError> {
    let mut keyed = Vec::with_capacity(diagnostics.len());
    for diagnostic in diagnostics {
        keyed.push((order_key(sources, source_units, diagnostic)?, diagnostic));
    }
    keyed.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    Ok(keyed
        .into_iter()
        .map(|(_, diagnostic)| diagnostic)
        .collect())
}

fn order_key(
    sources: &SourceMap,
    source_units: &[SourceUnit],
    diagnostic: &Diagnostic,
) -> Result<OrderKey, UnitDiagnosticOrderError> {
    let primary = diagnostic.primary_span();
    sources.slice(primary)?;
    let mut details = Vec::with_capacity(diagnostic.details().len());
    for detail in diagnostic.details() {
        details.push(match detail {
            DiagnosticDetail::Label(label) => {
                sources.slice(label.span())?;
                DetailKey::Label {
                    source: source_key(source_units, label.span())?.clone(),
                    start: label.span().start(),
                    end: label.span().end(),
                    message: label.message().to_owned(),
                }
            }
            DiagnosticDetail::Note(text) => DetailKey::Note(text.as_str().to_owned()),
            DiagnosticDetail::Help(text) => DetailKey::Help(text.as_str().to_owned()),
        });
    }
    Ok(OrderKey {
        source: source_key(source_units, primary)?.clone(),
        start: primary.start(),
        end: primary.end(),
        severity: match diagnostic.severity() {
            Severity::Error => 0,
            Severity::Warning => 1,
        },
        code: diagnostic.code().to_string(),
        message: diagnostic.message().to_owned(),
        details,
    })
}

fn source_key(
    source_units: &[SourceUnit],
    span: Span,
) -> Result<&SourceUnitKey, UnitDiagnosticOrderError> {
    source_units
        .iter()
        .find(|source_unit| source_unit.source_id() == span.source_id())
        .map(SourceUnit::key)
        .ok_or(UnitDiagnosticOrderError::SourceOutsideUnit(
            span.source_id(),
        ))
}
