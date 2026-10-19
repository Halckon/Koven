//! A2 preserves the two original P15 programs and their real lambda/body identities.
mod ast;
mod facts;
mod graph;
mod sources;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "SCOPE A2 INPUT label={} unit={unit} source={:?} golden={:?}",
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
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                let checked = facts::unit((c, &sources, &parsed, &input), n, t, o);
                r.query(
                    "A2 actual qualified callback/lambda/internal owner/complete cleanup",
                    &checked,
                );
                proof = checked.ok();
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            let checked = facts::single((c, &sources, &parsed, &input), n, t, o);
            r.query(
                "A2 actual callback/lambda/internal owner/complete cleanup",
                &checked,
            );
            proof = checked.ok();
        })
    };
    println!(
        "SCOPE A2 LOWER label={} unit={unit} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lowered?;
    let query = proof
        .as_ref()
        .ok_or_else(|| failure("A2 same-analysis callback/internal cleanup proof", &report))
        .and_then(|p| graph::check(c, &sources, &parsed, &input, p, &program));
    report.query(
        "A2 real FunctionAddress target/thunk/internal current Map/full incoming/protected uses",
        &query,
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "SCOPE A2 FINAL label={} unit={unit} report={report:#?}",
        c.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("A2 independent actual identity queries", report))
    }
}
#[test]
fn a2_original_lambda_body_sources_preserve_actual_identity() {
    let mut outcomes = Vec::new();
    for c in sources() {
        for unit in [false, true] {
            let result = observe(&c, unit);
            println!(
                "SCOPE A2 CASE label={} unit={unit} result={result:?}",
                c.label
            );
            outcomes.push((c.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
