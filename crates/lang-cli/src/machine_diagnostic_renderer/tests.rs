use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCode, DiagnosticCodeCatalog, Severity, SpanRole},
    source::{SourceError, SourceId, SourceMap, Span},
};
use serde_json::Value;

use super::{MachineDiagnosticError, render_machine_diagnostics};

fn code() -> DiagnosticCode {
    DiagnosticCodeCatalog::try_new(&["L9000"])
        .expect("test catalog")
        .resolve("L9000")
        .expect("test code")
}

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique source")
}

fn span(sources: &SourceMap, source_id: SourceId, start: usize, end: usize) -> Span {
    sources.span(source_id, start, end).expect("valid span")
}

fn diagnostic(sources: &SourceMap, message: &str, primary_span: Span) -> Diagnostic {
    Diagnostic::new(sources, Severity::Error, code(), message, primary_span)
        .expect("valid diagnostic")
}

#[test]
fn empty_collection_is_empty() {
    assert_eq!(
        render_machine_diagnostics(&SourceMap::new(), &[]).expect("empty render"),
        ""
    );
}

#[test]
fn renders_v1_primary_and_ordered_details() {
    let mut sources = SourceMap::new();
    let related = add_source(&mut sources, "related.ko", "xy\nz");
    let primary = add_source(&mut sources, "primary.ko", "a界\r\nβ\n");
    let mut diagnostic = diagnostic(&sources, "complete", span(&sources, primary, 1, 8));
    diagnostic
        .add_label(&sources, span(&sources, related, 0, 2), "relation")
        .expect("label");
    diagnostic.add_note("note text").expect("note");
    diagnostic.add_help("help text").expect("help");
    diagnostic
        .add_label(&sources, span(&sources, primary, 9, 9), "EOF")
        .expect("EOF label");

    let rendered = render_machine_diagnostics(&sources, &[diagnostic]).expect("render");
    assert_eq!(rendered.lines().count(), 1);
    let value: Value = serde_json::from_str(rendered.trim_end()).expect("JSON record");
    assert_eq!(value["schema"], "koven.diagnostic");
    assert_eq!(value["version"], 1);
    assert_eq!(value["severity"], "error");
    assert_eq!(value["code"], "L9000");
    assert_eq!(value["message"], "complete");
    assert_eq!(value["primary"]["source"], "primary.ko");
    assert_eq!(value["primary"]["byte_start"], 1);
    assert_eq!(value["primary"]["byte_end"], 8);
    assert_eq!(value["primary"]["start"]["line"], 1);
    assert_eq!(value["primary"]["start"]["column"], 2);
    assert_eq!(value["primary"]["end"]["line"], 2);
    assert_eq!(value["primary"]["end"]["column"], 2);
    assert_eq!(value["details"][0]["kind"], "label");
    assert_eq!(value["details"][1]["kind"], "note");
    assert_eq!(value["details"][2]["kind"], "help");
    assert_eq!(value["details"][3]["kind"], "label");
    assert_eq!(value["details"][3]["location"]["start"]["line"], 3);
    assert_eq!(value["details"][3]["location"]["start"]["column"], 1);
}

#[test]
fn source_and_message_are_json_escaped_without_path_rewriting() {
    let mut sources = SourceMap::new();
    let source = add_source(&mut sources, "virtual\\name\r\n.ko", "x");
    let diagnostic = diagnostic(
        &sources,
        "quote \" and slash \\",
        span(&sources, source, 0, 1),
    );

    let rendered = render_machine_diagnostics(&sources, &[diagnostic]).expect("render");
    assert!(rendered.contains("virtual\\\\name\\r\\n.ko"), "{rendered}");
    let value: Value = serde_json::from_str(rendered.trim_end()).expect("JSON record");
    assert_eq!(value["primary"]["source"], "virtual\\name\r\n.ko");
    assert_eq!(value["message"], "quote \" and slash \\");
}

#[test]
fn warning_severity_uses_the_stable_wire_spelling() {
    let mut sources = SourceMap::new();
    let source = add_source(&mut sources, "warning.ko", "x");
    let diagnostic = Diagnostic::new(
        &sources,
        Severity::Warning,
        code(),
        "warning message",
        span(&sources, source, 0, 1),
    )
    .expect("valid warning");

    let rendered = render_machine_diagnostics(&sources, &[diagnostic]).expect("render");
    let value: Value = serde_json::from_str(rendered.trim_end()).expect("JSON record");
    assert_eq!(value["severity"], "warning");
}

#[test]
fn output_is_deterministic_across_input_and_source_load_order() {
    let mut first_sources = SourceMap::new();
    let first_b = add_source(&mut first_sources, "b.ko", "b");
    let first_a = add_source(&mut first_sources, "a.ko", "a");
    let first_diagnostics = [
        diagnostic(&first_sources, "b", span(&first_sources, first_b, 0, 1)),
        diagnostic(&first_sources, "a", span(&first_sources, first_a, 0, 1)),
    ];

    let mut second_sources = SourceMap::new();
    let second_a = add_source(&mut second_sources, "a.ko", "a");
    let second_b = add_source(&mut second_sources, "b.ko", "b");
    let second_diagnostics = [
        diagnostic(&second_sources, "a", span(&second_sources, second_a, 0, 1)),
        diagnostic(&second_sources, "b", span(&second_sources, second_b, 0, 1)),
    ];

    let first =
        render_machine_diagnostics(&first_sources, &first_diagnostics).expect("first render");
    let repeat =
        render_machine_diagnostics(&first_sources, &first_diagnostics).expect("repeat render");
    let second =
        render_machine_diagnostics(&second_sources, &second_diagnostics).expect("second render");
    assert_eq!(first, second);
    assert_eq!(first, repeat);
    assert_eq!(first.lines().count(), 2);
}

#[test]
fn foreign_source_map_fails_without_output() {
    let mut owner = SourceMap::new();
    let source = add_source(&mut owner, "owner.ko", "x");
    let diagnostic = diagnostic(&owner, "foreign", span(&owner, source, 0, 1));
    let foreign = SourceMap::new();

    assert!(matches!(
        render_machine_diagnostics(&foreign, &[diagnostic]),
        Err(MachineDiagnosticError::Diagnostic(
            lang_frontend::diagnostic::DiagnosticError::InvalidSpan {
                role: SpanRole::Primary,
                source: SourceError::InvalidSourceId { .. },
            }
        ))
    ));
}
