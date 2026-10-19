//! P24 keeps the original Unit member receiver and independent Borrow Map parameter.
mod ast;
mod facts;
mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    provider: bool,
}
pub(crate) fn sources() -> [Case; 2] {
    [
        Case {
            label: "P24-for",
            provider: true,
            source: include_str!("../../../tests/fixtures/map-provider-scope-0331/P24-for.ko"),
            golden: include_str!("../../../tests/fixtures/map-provider-scope-0331/P24-for.stdout"),
        },
        Case {
            label: "P24-control",
            provider: false,
            source: include_str!("../../../tests/fixtures/map-provider-scope-0331/P24-control.ko"),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-scope-0331/P24-control.stdout"
            ),
        },
    ]
}
fn observe(c: &Case) -> Result<(), Failure> {
    println!(
        "SCOPE MEMBER INPUT label={} unit=true source={:?} golden={:?}",
        c.label, c.source, c.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", c.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = ast::input(c, &sources, &parsed)?;
    let mut report = Report::default();
    let mut proof = None;
    let mut observed = false;
    let lowered = pipeline::unit_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        |n, t, o, r| {
            observed = true;
            let result = facts::check(c, &parsed, &input, n, t, o);
            r.query(
                "P24 actual member receiver/parameter/call/source plan/complete cleanup",
                &result,
            );
            proof = result.ok();
        },
    );
    println!(
        "SCOPE MEMBER LOWER label={} unit=true observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    need(
        observed,
        "P24 original same-analysis callback was reached",
        c.label,
    )?;
    let (program, entry) = lowered?;
    let checked = proof
        .as_ref()
        .ok_or_else(|| failure("P24 same-analysis original facts", &report))
        .and_then(|proof| graph::check(c, &parsed, &input, proof, &program, entry));
    report.query(
        "P24 actual receiver/caller owner/full provider incoming and cleanup",
        &checked,
    );
    let verified = crate::ssa::verify::verify_program(&program).map_err(|e| failure("P24 SSA", e));
    report.query("P24 full original SSA verifier", &verified);
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .map(|_| ())
        .map_err(|e| failure("P24 LLVM render/verify", e));
    report.query("P24 full original LLVM render/verify", &llvm);
    println!(
        "SCOPE MEMBER FINAL label={} unit=true report={report:#?}",
        c.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("P24 original positive regression", report))
    }
}
#[test]
fn unit_original_member_map_parameter_preserves_full_identity() {
    let outcomes = sources()
        .into_iter()
        .map(|c| {
            let result = observe(&c);
            println!(
                "SCOPE MEMBER CASE label={} unit=true result={result:?}",
                c.label
            );
            (c.label, result)
        })
        .collect::<Vec<_>>();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|(_, r)| r.is_ok()), "{outcomes:#?}");
}
