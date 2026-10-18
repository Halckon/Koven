//! Ordinary well-typed source staging neighbors; no mutated products or injected failures.
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
    query_inputs::one,
};
use lang_frontend::{
    lexer::lex,
    parser::{Statement, parse_file},
    source::SourceMap,
};

fn observe(text: &str, unit: bool) -> Result<bool, Failure> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let span =
        one(
            parsed.ast().statements().iter().filter_map(|(_, n)| {
                matches!(n.payload(), Statement::For { .. }).then_some(n.span())
            }),
            "one actual boundary For",
        )?;
    let mut report = Report::default();
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |_, _, _, _| {},
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |_, _, _, _| {})
    };
    // Failure preserves the original LoweringError Debug value before adding source location.
    // Match the exact kind and original For span, not an arbitrary Err or stage failure.
    let expected = format!("(LoweringError {{ kind: UnsupportedNode, span: Some({span:?}) }},");
    let exact =
        matches!(&lowered, Err(e) if e.stage == "lower" && e.original.starts_with(&expected));
    println!(
        "RESOURCE BOUNDARY unit={unit} expected={expected} report={report:#?} actual={:?}",
        lowered.as_ref().map(|(_, entry)| entry)
    );
    Ok(report.passed() && exact)
}

#[test]
fn resource_scope_staging_neighbors_keep_exact_for_boundary() {
    let cases = [
        (
            "continue",
            include_str!(
                "../../../../tests/fixtures/map-provider-resource-0331/boundary-continue.ko"
            ),
        ),
        (
            "nullable",
            include_str!(
                "../../../../tests/fixtures/map-provider-resource-0331/boundary-nullable.ko"
            ),
        ),
    ];
    let mut results = Vec::new();
    for (case, text) in cases {
        for unit in [false, true] {
            results.push((case, unit, observe(text, unit)));
        }
    }
    assert!(
        results.len() == 4 && results.iter().all(|(_, _, r)| matches!(r, Ok(true))),
        "all ordinary boundary sources must reach exact lowering rejection: {results:#?}"
    );
}
