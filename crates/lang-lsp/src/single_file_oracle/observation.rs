//! Frozen Legacy behavior from independently reviewed ordinary source observations.

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    source::{SourceMap, Span},
};
use lsp_types::{Range, Uri};
use serde_json::{Value, json};

use super::{fixtures, position};
use crate::{
    analysis,
    definition::DefinitionIndex,
    diagnostic_adapter::convert_diagnostics,
    position_adapter::{byte_offset, span_range},
};

#[test]
fn matches_frozen_legacy_recovery_diagnostics_and_every_definition_offset() {
    let frozen: Value = serde_json::from_str(include_str!("fixtures/legacy_recovery_v1.json"))
        .expect("reviewed frozen Legacy observations");
    assert_eq!(frozen["schema_version"], json!(1));
    let expected = frozen["fixtures"]
        .as_array()
        .expect("eleven frozen fixtures");
    let inputs = fixtures();
    assert_eq!(inputs.len(), 11);
    assert_eq!(expected.len(), inputs.len());
    for ((label, source), expected) in inputs.into_iter().zip(expected) {
        let uri: Uri = format!("file:///{label}.ko").parse().expect("fixture URI");
        assert_eq!(expected["label"], json!(label));
        assert_eq!(expected["uri"], json!(uri.as_str()));
        assert_eq!(expected["source_utf8"], json!(source));
        assert_eq!(expected["source_byte_length"], json!(source.len()));
        let actual = analysis::analyze(uri.as_str(), source).expect("production analysis");
        let actual_snapshot = snapshot(
            &actual.sources,
            &actual.diagnostics,
            &actual.definitions,
            &uri,
            source,
        );
        assert_eq!(
            actual_snapshot, expected["manual_and_production_equal_snapshot"],
            "{label}: complete frozen raw/LSP diagnostics and ordered navigation"
        );
        // manual_stage_diagnostics_only is historical metadata, not an assertion
        // about future Unit stages or executable qualification.
    }
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
