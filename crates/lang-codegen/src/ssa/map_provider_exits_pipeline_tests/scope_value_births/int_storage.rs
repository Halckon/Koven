//! P06 original Int storage programs, independently queried through both public entry paths.
mod ast;
mod facts;
mod graph;
mod sources;
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};
fn observe(c: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "SCOPE B INT INPUT label={} unit={unit} source={:?} golden={:?}",
        c.label, c.source, c.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", c.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let p = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", p.diagnostics())?;
    let input = ast::input(c, &sources, &p)?;
    let mut report = Report::default();
    let mut proof = None;
    let lower = if unit {
        pipeline::unit_observed(&sources, &p, "provider.ko", &mut report, |n, t, o, r| {
            let result = facts::unit((c, &sources, &p, &input), n, t, o);
            r.query(
                "Int actual qualified creation/Copy types/consume/reader/current cleanup",
                &result,
            );
            proof = result.ok();
        })
    } else {
        pipeline::single_observed(&sources, &p, &mut report, |n, t, o, r| {
            let result = facts::single((c, &sources, &p, &input), n, t, o);
            r.query(
                "Int actual creation/Copy types/consume/reader/current cleanup",
                &result,
            );
            proof = result.ok();
        })
    };
    println!(
        "SCOPE B INT LOWER label={} unit={unit} report={report:#?} outcome={:?}",
        c.label,
        lower.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lower?;
    let query = proof
        .as_ref()
        .ok_or_else(|| failure("Int same-analysis cleanup proof", &report))
        .and_then(|proof| graph::check(c, &p, &input, proof, &program));
    report.query(
        "Int actual factory/Copy comparisons/current owner/full incoming/all protected uses",
        &query,
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "SCOPE B INT FINAL label={} unit={unit} report={report:#?}",
        c.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("Int actual deep identity queries", report))
    }
}
#[test]
fn original_int_storage_sources_preserve_actual_identity() {
    let mut outcomes = Vec::new();
    for c in sources() {
        for unit in [false, true] {
            let result = observe(&c, unit);
            println!(
                "SCOPE B INT CASE label={} unit={unit} result={result:?}",
                c.label
            );
            outcomes.push((c.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
