//! Ordinary source observations only; this module contains no expected golden or update mode.

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    source::{SourceMap, Span},
};
use lsp_types::{Range, Uri};
use serde_json::{Value, json};

use super::{ManualAnalysis, assert_diagnostics, fixtures, manual, position};
use crate::{
    analysis,
    definition::DefinitionIndex,
    diagnostic_adapter::convert_diagnostics,
    position_adapter::{byte_offset, span_range},
};

#[test]
fn observes_complete_legacy_recovery_before_oracle_freeze() {
    let mut observations = Vec::new();
    for (label, source) in fixtures() {
        let uri: Uri = format!("file:///{label}.ko").parse().expect("fixture URI");
        let expected = manual(uri.as_str(), source);
        let actual = analysis::analyze(uri.as_str(), source).expect("production analysis");
        assert_diagnostics(
            &actual.sources,
            &actual.diagnostics,
            &expected.sources,
            &expected.diagnostics,
            uri.as_str(),
        );
        let expected_snapshot = snapshot(
            &expected.sources,
            &expected.diagnostics,
            &expected.definitions,
            &uri,
            source,
        );
        let actual_snapshot = snapshot(
            &actual.sources,
            &actual.diagnostics,
            &actual.definitions,
            &uri,
            source,
        );
        assert_eq!(actual_snapshot, expected_snapshot, "{label}: full snapshot");
        observations.push(json!({
            "label": label,
            "uri": uri.as_str(),
            "source_utf8": source,
            "source_byte_length": source.len(),
            "manual_stage_diagnostics_only": stage_diagnostics(&expected, &uri),
            "manual_and_production_equal_snapshot": expected_snapshot,
        }));
    }
    assert_eq!(observations.len(), 11);
    // Emit only after every independent/manual comparison succeeds. Never write expectations.
    println!(
        "LSP_LEGACY_ORACLE_OBSERVATION_V1={}",
        serde_json::to_string(&json!({"schema_version": 1, "fixtures": observations}))
            .expect("complete observation JSON")
    );
}

fn snapshot(
    sources: &SourceMap,
    diagnostics: &[Diagnostic],
    definitions: &DefinitionIndex,
    uri: &Uri,
    source: &str,
) -> Value {
    let source_id = definitions.source_id();
    assert_eq!(
        sources.source_name(source_id).expect("source owner"),
        uri.as_str()
    );
    assert_eq!(
        sources.source_text(source_id).expect("source bytes"),
        source
    );
    let mut navigation = Vec::new();
    for offset in (0..=source.len()).filter(|offset| source.is_char_boundary(*offset)) {
        let targets = definitions.targets_at(offset);
        for target in targets {
            let range = span_range(sources, *target).expect("target range");
            assert_eq!(
                range,
                Range::new(
                    position(source, target.start()),
                    position(source, target.end())
                )
            );
            assert_eq!(
                byte_offset(sources, target.source_id(), range.start).expect("target start"),
                Some(target.start())
            );
        }
        // Keep empty target lists and CRLF terminator byte boundaries. They are not all LSP cursors.
        navigation.push(json!({
            "byte_offset": offset,
            "targets": targets.iter().map(|span| span_value(sources, *span, uri)).collect::<Vec<_>>(),
        }));
    }
    json!({
        "raw_diagnostics": diagnostic_values(sources, diagnostics, uri),
        "lsp_diagnostics": convert_diagnostics(sources, uri, diagnostics).expect("mapped diagnostics"),
        "definitions_by_utf8_boundary": navigation,
    })
}

fn stage_diagnostics(analysis: &ManualAnalysis, uri: &Uri) -> Vec<Value> {
    analysis
        .stage_diagnostics
        .iter()
        .map(|(stage, diagnostics)| {
            json!({
                "stage": stage,
                "diagnostics": diagnostic_values(&analysis.sources, diagnostics, uri),
            })
        })
        .collect()
}

fn diagnostic_values(sources: &SourceMap, diagnostics: &[Diagnostic], uri: &Uri) -> Vec<Value> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let details = diagnostic
                .details()
                .iter()
                .map(|detail| match detail {
                    DiagnosticDetail::Label(label) => json!({
                        "kind": "label",
                        "message": label.message(),
                        "span": span_value(sources, label.span(), uri),
                    }),
                    DiagnosticDetail::Note(note) => json!({"kind": "note", "text": note.as_str()}),
                    DiagnosticDetail::Help(help) => json!({"kind": "help", "text": help.as_str()}),
                })
                .collect::<Vec<_>>();
            json!({
                "severity": match diagnostic.severity() {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                },
                "code": diagnostic.code().to_string(),
                "message": diagnostic.message(),
                "primary_span": span_value(sources, diagnostic.primary_span(), uri),
                "details": details,
            })
        })
        .collect()
}

fn span_value(sources: &SourceMap, span: Span, uri: &Uri) -> Value {
    let source_name = sources
        .source_name(span.source_id())
        .expect("span source owner");
    assert_eq!(source_name, uri.as_str());
    json!({
        "source_name": source_name,
        "start": span.start(),
        "end": span.end(),
        "source_slice": sources.slice(span).expect("same-map span slice"),
        "utf16_range": span_range(sources, span).expect("same-map span range"),
    })
}
