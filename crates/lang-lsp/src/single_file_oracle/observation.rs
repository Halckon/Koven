//! 固定 Unit 完整观测；按独立 producer 审查迁移，不兼容旧 checker 遗漏。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    name_resolution::SourceUnitId,
    source::{SourceId, SourceMap, Span},
};
use lsp_types::{PublishDiagnosticsParams, Range, Uri};
use serde_json::{Value, json};

use super::{RECOVERY, fixtures, position};
use crate::{
    analysis,
    diagnostic_adapter::convert_unit_diagnostics,
    position_adapter::{byte_offset, span_range},
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

fn frozen() -> Value {
    let frozen: Value = serde_json::from_str(include_str!("fixtures/unit_recovery_v1.json"))
        .expect("reviewed frozen Unit observations");
    assert_eq!(frozen["schema_version"], json!(1));
    assert_eq!(
        frozen["pipeline"],
        json!("production_compilation_unit_recovery")
    );
    assert_eq!(frozen["fixtures"].as_array().expect("fixtures").len(), 11);
    frozen
}

#[test]
fn matches_frozen_unit_recovery_diagnostics_and_every_definition_offset() {
    let frozen = frozen();
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
        let (source_id, actual) =
            analysis::analyze(uri.as_str(), source).expect("production Unit analysis");
        assert_eq!(
            snapshot(&actual, source_id, &uri, source),
            expected["snapshot"],
            "{label}: complete frozen raw/LSP diagnostics and ordered navigation"
        );
        // 固定期望只承载已审真实 Unit 输出，不由当前分析生成或接受旧新两种结果。
        // 旧 manual-stage metadata 留历史证据，不作为 Unit 阶段或执行资格。
    }
}

pub(super) fn source_unit(analysis: &UnitAnalysisSnapshot, source_id: SourceId) -> SourceUnitId {
    let units = analysis.name_snapshot().names().index().source_units();
    assert_eq!(units.len(), 1, "one explicitly registered document");
    units
        .iter()
        .find(|unit| unit.source_id() == source_id)
        .expect("registered SourceId retained in Unit snapshot")
        .id()
}

fn snapshot(
    analysis: &UnitAnalysisSnapshot,
    source_id: SourceId,
    uri: &Uri,
    source: &str,
) -> Value {
    let sources = analysis.name_snapshot().sources();
    let source_unit = source_unit(analysis, source_id);
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
        let targets = analysis.definitions().targets_at(source_unit, offset);
        for target in targets {
            assert_eq!(target.source_unit, source_unit, "same document Unit owner");
            assert_eq!(
                target.span.source_id(),
                source_id,
                "registered source owner"
            );
            let range = span_range(sources, target.span).expect("target range");
            assert_eq!(
                range,
                Range::new(
                    position(source, target.span.start()),
                    position(source, target.span.end())
                )
            );
            assert_eq!(
                byte_offset(sources, source_id, range.start).expect("target start"),
                Some(target.span.start())
            );
        }
        // Keep empty target lists and CRLF terminator byte boundaries. They are not all LSP cursors.
        navigation.push(json!({
            "byte_offset": offset,
            "targets": targets.iter().map(|target| {
                span_value(sources, source_id, target.span, uri)
            }).collect::<Vec<_>>(),
        }));
    }
    let mut publications =
        convert_unit_diagnostics(sources, &[(source_id, uri.clone())], analysis.diagnostics())
            .expect("mapped Unit diagnostics");
    assert_eq!(publications.len(), 1);
    json!({
        "raw_diagnostics": diagnostic_values(sources, source_id, analysis.diagnostics(), uri),
        "lsp_diagnostics": publications.pop().expect("document diagnostics"),
        "definitions_by_utf8_boundary": navigation,
    })
}

fn diagnostic_values(
    sources: &SourceMap,
    source_id: SourceId,
    diagnostics: &[Diagnostic],
    uri: &Uri,
) -> Vec<Value> {
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
                        "span": span_value(sources, source_id, label.span(), uri),
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
                "primary_span": span_value(sources, source_id, diagnostic.primary_span(), uri),
                "details": details,
            })
        })
        .collect()
}

fn span_value(sources: &SourceMap, source_id: SourceId, span: Span, uri: &Uri) -> Value {
    assert_eq!(
        span.source_id(),
        source_id,
        "registered span source identity"
    );
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

pub(crate) fn assert_recovery_snapshot(
    analysis: &UnitAnalysisSnapshot,
    source_id: SourceId,
    uri: &Uri,
    source: &str,
) {
    assert_eq!(
        snapshot(analysis, source_id, uri, source),
        expected_recovery_snapshot(uri, source),
        "committed raw/LSP diagnostics and every ordered navigation boundary"
    );
}

pub(crate) fn recovery_publication(
    uri: &Uri,
    version: i32,
    source: &str,
) -> PublishDiagnosticsParams {
    let expected = expected_recovery_snapshot(uri, source);
    PublishDiagnosticsParams::new(
        uri.clone(),
        serde_json::from_value(expected["lsp_diagnostics"].clone())
            .expect("frozen complete LSP diagnostics"),
        Some(version),
    )
}

/// 仅重基已冻结 recovery 观测；没有 checker、SourceMap 或生产分析调用。
fn expected_recovery_snapshot(uri: &Uri, source: &str) -> Value {
    let prefix = source
        .strip_suffix(RECOVERY)
        .expect("unchanged recovery source");
    assert!(
        ["", "\r\n", "\r\n\r\n", "\r\n\r\n\r\n"].contains(&prefix),
        "only the four reviewed protocol CRLF prefixes"
    );
    let frozen = frozen();
    let fixture = &frozen["fixtures"][0];
    assert_eq!(fixture["label"], json!("recovery"));
    assert_eq!(fixture["uri"], json!("file:///recovery.ko"));
    assert_eq!(fixture["source_utf8"], json!(RECOVERY));
    assert_eq!(fixture["source_byte_length"], json!(RECOVERY.len()));
    let mut expected = fixture["snapshot"].clone();
    let bytes = u64::try_from(prefix.len()).expect("prefix bytes");
    let lines = bytes / 2;
    for diagnostic in expected["raw_diagnostics"]
        .as_array_mut()
        .expect("raw diagnostics")
    {
        rebase_span(&mut diagnostic["primary_span"], uri, bytes, lines);
        for detail in diagnostic["details"]
            .as_array_mut()
            .expect("ordered details")
        {
            if detail["kind"] == "label" {
                rebase_span(&mut detail["span"], uri, bytes, lines);
            }
        }
    }
    for diagnostic in expected["lsp_diagnostics"]
        .as_array_mut()
        .expect("LSP diagnostics")
    {
        rebase_range(&mut diagnostic["range"], lines);
        if let Some(related) = diagnostic.get_mut("relatedInformation") {
            for detail in related.as_array_mut().expect("ordered related information") {
                let location = &mut detail["location"];
                assert_eq!(location["uri"], json!("file:///recovery.ko"));
                location["uri"] = json!(uri.as_str());
                rebase_range(&mut location["range"], lines);
            }
        }
    }
    let navigation = expected["definitions_by_utf8_boundary"]
        .as_array_mut()
        .expect("ordered navigation");
    for entry in navigation.iter_mut() {
        entry["byte_offset"] = json!(entry["byte_offset"].as_u64().expect("offset") + bytes);
        for target in entry["targets"].as_array_mut().expect("ordered targets") {
            rebase_span(target, uri, bytes, lines);
        }
    }
    // The original offset zero moves to `bytes`; add each new prefix boundary once.
    let prefix_boundaries = (0..bytes).map(|offset| json!({"byte_offset": offset, "targets": []}));
    drop(navigation.splice(0..0, prefix_boundaries));
    expected
}

fn rebase_span(span: &mut Value, uri: &Uri, bytes: u64, lines: u64) {
    assert_eq!(span["source_name"], json!("file:///recovery.ko"));
    span["source_name"] = json!(uri.as_str());
    for edge in ["start", "end"] {
        span[edge] = json!(span[edge].as_u64().expect("byte boundary") + bytes);
    }
    rebase_range(&mut span["utf16_range"], lines);
}

fn rebase_range(range: &mut Value, lines: u64) {
    for edge in ["start", "end"] {
        range[edge]["line"] = json!(range[edge]["line"].as_u64().expect("line") + lines);
    }
}

/// 独立用源码字节与 position 核对重基，再还原坐标比较全部未变字段、计数与顺序。
pub(super) fn assert_recovery_rebasing() {
    let frozen = frozen();
    let original = &frozen["fixtures"][0]["snapshot"];
    let original_uri: Uri = "file:///recovery.ko".parse().expect("frozen URI");
    assert_eq!(
        expected_recovery_snapshot(&original_uri, RECOVERY),
        *original
    );
    for prefix in ["", "\r\n", "\r\n\r\n", "\r\n\r\n\r\n"] {
        let uri: Uri = "file:///rebased-recovery.ko".parse().expect("protocol URI");
        let source = format!("{prefix}{RECOVERY}");
        let expected = expected_recovery_snapshot(&uri, &source);
        let mut restored = expected.clone();
        let original_raw = original["raw_diagnostics"]
            .as_array()
            .expect("raw diagnostics");
        let raw = expected["raw_diagnostics"]
            .as_array()
            .expect("rebased raw diagnostics");
        assert_eq!(raw.len(), original_raw.len());
        for (index, (actual, original)) in raw.iter().zip(original_raw).enumerate() {
            assert_rebased_span(
                &original["primary_span"],
                &actual["primary_span"],
                &uri,
                &source,
                prefix.len(),
            );
            restored["raw_diagnostics"][index]["primary_span"] = original["primary_span"].clone();
            let details = actual["details"].as_array().expect("details");
            assert_eq!(
                details.len(),
                original["details"]
                    .as_array()
                    .expect("original details")
                    .len()
            );
            for (detail_index, detail) in details.iter().enumerate() {
                if detail["kind"] == "label" {
                    let original_span = &original["details"][detail_index]["span"];
                    assert_rebased_span(
                        original_span,
                        &detail["span"],
                        &uri,
                        &source,
                        prefix.len(),
                    );
                    restored["raw_diagnostics"][index]["details"][detail_index]["span"] =
                        original_span.clone();
                }
            }
        }
        let lsp = expected["lsp_diagnostics"]
            .as_array()
            .expect("LSP diagnostics");
        assert_eq!(
            lsp.len(),
            original["lsp_diagnostics"]
                .as_array()
                .expect("original LSP")
                .len()
        );
        assert_eq!(lsp.len(), raw.len());
        for (index, diagnostic) in lsp.iter().enumerate() {
            assert_eq!(
                diagnostic["range"],
                raw[index]["primary_span"]["utf16_range"]
            );
            restored["lsp_diagnostics"][index]["range"] =
                original["lsp_diagnostics"][index]["range"].clone();
            let labels = raw[index]["details"]
                .as_array()
                .expect("details")
                .iter()
                .filter(|detail| detail["kind"] == "label")
                .collect::<Vec<_>>();
            if let Some(related) = diagnostic.get("relatedInformation") {
                let related = related.as_array().expect("related information");
                assert_eq!(related.len(), labels.len());
                for (related_index, (detail, label)) in related.iter().zip(labels).enumerate() {
                    assert_eq!(detail["location"]["uri"], json!(uri.as_str()));
                    assert_eq!(detail["location"]["range"], label["span"]["utf16_range"]);
                    let location = &mut restored["lsp_diagnostics"][index]["relatedInformation"]
                        [related_index]["location"];
                    let original_location = &original["lsp_diagnostics"][index]["relatedInformation"]
                        [related_index]["location"];
                    location["uri"] = original_location["uri"].clone();
                    location["range"] = original_location["range"].clone();
                }
            } else {
                assert!(labels.is_empty());
            }
        }
        let navigation = expected["definitions_by_utf8_boundary"]
            .as_array()
            .expect("navigation");
        let offsets = (0..=source.len())
            .filter(|offset| source.is_char_boundary(*offset))
            .collect::<Vec<_>>();
        assert_eq!(navigation.len(), offsets.len());
        for (entry, offset) in navigation.iter().zip(offsets) {
            assert_eq!(entry["byte_offset"], json!(offset));
        }
        for (offset, entry) in navigation[..prefix.len()].iter().enumerate() {
            assert_eq!(*entry, json!({"byte_offset": offset, "targets": []}));
        }
        let original_navigation = original["definitions_by_utf8_boundary"]
            .as_array()
            .expect("original navigation");
        assert_eq!(navigation.len(), original_navigation.len() + prefix.len());
        let restored_navigation = restored["definitions_by_utf8_boundary"]
            .as_array_mut()
            .expect("restored navigation");
        drop(restored_navigation.drain(..prefix.len()));
        for (index, (actual, original)) in navigation[prefix.len()..]
            .iter()
            .zip(original_navigation)
            .enumerate()
        {
            let targets = actual["targets"].as_array().expect("targets");
            let original_targets = original["targets"].as_array().expect("original targets");
            assert_eq!(targets.len(), original_targets.len());
            for (target_index, (target, original_target)) in
                targets.iter().zip(original_targets).enumerate()
            {
                assert_rebased_span(original_target, target, &uri, &source, prefix.len());
                restored_navigation[index]["targets"][target_index] = original_target.clone();
            }
            assert_eq!(
                actual["byte_offset"].as_u64().expect("rebased offset"),
                original["byte_offset"].as_u64().expect("original offset")
                    + u64::try_from(prefix.len()).expect("prefix")
            );
            restored_navigation[index]["byte_offset"] = original["byte_offset"].clone();
        }
        assert_eq!(
            restored, *original,
            "all non-coordinate fields, counts and ordering unchanged"
        );
    }
}

fn assert_rebased_span(original: &Value, actual: &Value, uri: &Uri, source: &str, prefix: usize) {
    let start = usize::try_from(original["start"].as_u64().expect("original start"))
        .expect("start")
        + prefix;
    let end =
        usize::try_from(original["end"].as_u64().expect("original end")).expect("end") + prefix;
    assert_eq!(json!(&source[start..end]), original["source_slice"]);
    let mut expected = original.clone();
    expected["source_name"] = json!(uri.as_str());
    expected["start"] = json!(start);
    expected["end"] = json!(end);
    expected["utf16_range"] = json!(Range::new(position(source, start), position(source, end)));
    assert_eq!(*actual, expected);
}
