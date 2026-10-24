//! A1 frozen scope programs: explicit no-For controls and complete ordinary provider identities.
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
fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "SCOPE A1 INPUT label={} rooted={unit} source={:?} golden={:?}",
        case.label, case.source, case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = ast::input(case, &sources, &parsed)?;
    let mut report = Report::default();
    let lowered = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |n, t, o, r| {
            r.query(
                "scope A1 qualified actual owner/parameter/body/complete cleanup",
                &facts::unit((case, &sources, &parsed, &input), n, t, o),
            );
        },
    );
    println!(
        "SCOPE A1 LOWER label={} rooted={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lowered?;
    report.query(
        "scope A1 independent SSA all incoming/current loans/protected uses/endpoints",
        &graph::check(case, &sources, &parsed, &input, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "SCOPE A1 FINAL label={} rooted={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("scope A1 independent identity queries", report))
    }
}
#[test]
fn a1_original_scope_sources_preserve_actual_identity() {
    let mut outcomes = Vec::new();
    let cases = sources();
    assert_eq!(cases.len(), 14);
    for c in cases {
        for unit in [false, true] {
            let r = observe(&c, unit);
            println!("SCOPE A1 CASE label={} rooted={unit} result={r:?}", c.label);
            outcomes.push((c.label, unit, r));
        }
    }
    assert_eq!(outcomes.len(), 28);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
