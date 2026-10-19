//! Ordinary intrinsic births and Unit FunctionValue results reach the original providers.
mod ast;
mod facts;
mod graph;
mod sources;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, Kind, Shape, sources};
fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "BIRTH INPUT label={} unit={unit} source={:?} golden={:?}",
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
    let mut proof = None;
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                source_forms::facts::unit(&sources, &parsed, n, t, o, r);
                let checked = facts::unit((case, &sources, &parsed, &input), n, t, o);
                r.query(
                    "birth same-analysis qualified actual creation/consume/invoke and owner",
                    &checked,
                );
                proof = Some(checked);
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::single(&sources, &parsed, n, t, o, r);
            let checked = facts::single((case, &sources, &parsed, &input), n, t, o);
            r.query(
                "birth same-analysis actual intrinsic/source/owner/cleanup",
                &checked,
            );
            proof = Some(checked);
        })
    };
    println!(
        "BIRTH LOWER label={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    let proof = proof.ok_or_else(|| failure("birth actual analysis observer", "not called"))??;
    report.query(
        "birth actual SSA producer/current owner/full incoming/protected uses/Drop",
        &graph::check(case, &sources, &parsed, &input, &proof, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "BIRTH FINAL label={} unit={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("birth independent FE/SSA proof", report))
    }
}
#[test]
fn original_intrinsic_and_unit_function_value_births_keep_exact_owner_identity() {
    let cases = sources();
    assert_eq!(cases.len(), 16);
    // Preserve all six Single FunctionValue goals for 0336 without turning
    // their existing ordinary-call prerequisite into a rejection oracle.
    for c in cases.iter().filter(|c| c.kind == Kind::FunctionValue) {
        println!(
            "BIRTH SINGLE FUNCTION PREREQUISITE label={} source={:?} golden={:?}",
            c.label, c.source, c.golden
        );
    }
    let mut outcomes = Vec::new();
    for case in cases {
        for unit in if case.kind == Kind::FunctionValue {
            vec![true]
        } else {
            vec![false, true]
        } {
            let result = observe(&case, unit);
            println!(
                "BIRTH CASE label={} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label, unit, result));
        }
    }
    assert_eq!(outcomes.len(), 26);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
