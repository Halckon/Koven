//! Argument-bearing ordinary factories form the actual named provider owner.
mod ast;
mod facts;
mod generic;
mod graph;
mod sources;
use super::fixture;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms, temporary_arguments,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) use generic::{
    native_sources as generic_sources, prerequisite_sources as generic_prerequisite_sources,
};
pub(crate) use sources::{Case, sources};

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "LOCAL ARG INPUT label={} unit={unit} source={:?} golden={:?}",
        case.label, case.source, case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
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
                let checked = facts::unit(case, &sources, &parsed, &input, n, t, o);
                r.query(
                    "local argument exact qualified Call, delivery, birth and provider owner",
                    &checked,
                );
                if case.label.starts_with("generic-wrapper-") {
                    r.query(
                        "generic named wrapper qualified forwarding",
                        &generic::wrapper::unit(&sources, &parsed, n, t, o),
                    );
                }
                proof = Some(checked);
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::single(&sources, &parsed, n, t, o, r);
            let checked = facts::single(case, &sources, &parsed, &input, n, t, o);
            r.query(
                "local argument exact Call, delivery, birth and all owner incoming",
                &checked,
            );
            if case.label.starts_with("generic-wrapper-") {
                r.query(
                    "generic named wrapper forwarding",
                    &generic::wrapper::single(&sources, &parsed, n, t, o),
                );
            }
            proof = Some(checked);
        })
    };
    println!(
        "LOCAL ARG LOWER label={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    let proof = proof.ok_or_else(|| failure("local argument observer", "not called"))??;
    report.query(
        "local argument actual SSA current owner, complete incoming and protected uses",
        &graph::check(case, &sources, &parsed, &input, &proof, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("local argument independent queries", report));
    }
    Ok(())
}

#[test]
fn selected_argument_factory_forms_named_owner_in_both_pipelines() {
    let cases = sources();
    assert_eq!(cases.len(), 18);
    let mut outcomes = Vec::new();
    for case in cases {
        for unit in if case.unit_only {
            vec![true]
        } else {
            vec![false, true]
        } {
            let result = observe(&case, unit);
            println!(
                "LOCAL ARG CASE label={} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(outcomes.len(), 30);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}
