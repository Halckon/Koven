//! Resource local future-use corners: lexical cleanup after all later reads and markers.
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod facts;
mod graph;
mod sources;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "RESOURCE LOCAL FUTURE INPUT case={} unit={unit} source={:?} golden={:?}",
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
    let mut report = Report::default();
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "Resource future actual qualified plan count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
                        && owned.map_iterations().len() == typed.map_iterations().len(),
                    typed.map_iterations(),
                );
                println!(
                    "RESOURCE LOCAL FUTURE UNIT case={} formed={:#?} plans={:#?} drops={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.loans()
                );
            },
        )
    } else {
        pipeline::memory_document_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "Resource future actual qualified plan count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
                        && owned.map_iterations().len() == typed.map_iterations().len(),
                    typed.map_iterations(),
                );
                println!(
                    "RESOURCE LOCAL FUTURE UNIT case={} formed={:#?} plans={:#?} drops={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.loans()
                );
            },
        )
    };
    println!(
        "RESOURCE LOCAL FUTURE REPORT case={} unit={unit} report={report:#?}",
        case.label
    );
    let (program, _) = result?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    if !report.passed() {
        return Err(failure("original Resource future FE", report));
    }
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    graph::check(case, &sources, &parsed, &program)?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}
#[test]
fn resource_local_future_uses_preserve_lexical_owners() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!(
                "RESOURCE LOCAL FUTURE CASE {} unit={unit} result={result:?}",
                case.label
            );
            results.push((case.label.clone(), unit, result));
        }
    }
    assert!(
        results.len() == 48 && results.iter().all(|(_, _, r)| r.is_ok()),
        "{results:#?}"
    );
}
