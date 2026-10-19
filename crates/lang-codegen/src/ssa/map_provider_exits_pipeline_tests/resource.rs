//! Newly rebuilt finite Map<String, Resource> normal-exhaustion pipelines.
//! Resource values remain container-owned; the loop creates Shared component loans.
pub(super) mod continuation;
pub(super) mod continued;
mod identity;
pub(super) mod nullable;
pub(super) mod terminated;

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            "map-resource",
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.ko"),
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout"),
        ),
        (
            "mutable-map-resource",
            include_str!(
                "../../../tests/fixtures/map-provider-resource-0331/mutable-map-resource.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-resource-0331/mutable-map-resource.stdout"
            ),
        ),
    ]
}

fn observe(label: &str, text: &str, unit: bool, report: &mut Report) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let input = identity::ast(&sources, &parsed, label == "map-resource")?;
    let (program, entry) = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            report,
            |names, typed, owned, report| {
                report.query(
                    "resource original unit FE obligations",
                    &identity::unit(&input, &parsed, names, typed, owned),
                );
                report.check(
                    "unit one actual Resource Map provider",
                    typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
                    (typed.map_iterations(), owned.map_iterations()),
                );
            },
        )?
    } else {
        pipeline::single_observed(&sources, &parsed, report, |names, typed, owned, report| {
            report.query(
                "resource original single FE obligations",
                &identity::single(&input, &parsed, names, typed, owned),
            );
            report.check(
                "single one actual Resource Map provider",
                typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
                (typed.map_iterations(), owned.map_iterations()),
            );
        })?
    };
    report.query(
        "resource independent SSA incoming/current lexical drop",
        &identity::ssa(&program, &input, unit),
    );
    super::super::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
    report.check("SSA verifier", true, entry);
    let llvm = crate::llvm::render_verified_program(&program)
        .map_err(|e| failure("LLVM render/verify", e))?;
    report.check("LLVM render/verify", true, llvm.len());
    Ok(())
}

#[test]
fn resource_map_normal_sources_reach_verified_llvm_in_both_entries() {
    let mut results = Vec::new();
    for (label, text, _) in sources() {
        for unit in [false, true] {
            let mut report = Report::default();
            let outcome = observe(label, text, unit, &mut report);
            println!(
                "SPEC0331 RESOURCE case={label} unit={unit} outcome={outcome:?} report={report:#?}"
            );
            results.push((label, unit, report.passed() && outcome.is_ok()));
        }
    }
    assert!(
        results.len() == 4 && results.iter().all(|(_, _, passed)| *passed),
        "all four original Resource Map pipelines must pass: {results:?}"
    );
}
