//! Real local formation plus future uses: preserve every public FE/lowering observation.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod facts;
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
        "LOCAL FUTURE INPUT case={} unit={unit} source={:?} golden={:?}",
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
    // Queries record failures in Report; neither path turns a failed identity
    // query into an artificial lowering failure or prevents ordinary lowering.
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "local future original unit descriptor count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 },
                    typed.map_iterations(),
                );
                println!(
                    "LOCAL FUTURE UNIT FACTS case={} formed={:#?} plans={:#?} drops={:#?} deliveries={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries(),
                    owned.loans()
                );
            },
        )
    } else {
        pipeline::single_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::single(&sources, &parsed, names, typed, owned, report);
                facts::single(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "local future original single descriptor count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 },
                    typed.map_iterations(),
                );
                println!(
                    "LOCAL FUTURE SINGLE FACTS case={} formed={:#?} plans={:#?} drops={:#?} conditions={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.cleanup_conditions(),
                    owned.loans()
                );
            },
        )
    };
    println!(
        "LOCAL FUTURE LOWER case={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    let identity = graph::check(case, &sources, &parsed, &program);
    report.query("local future independent SSA identities", &identity);
    let verified =
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e));
    report.query("local future actual SSA verifier", &verified);
    let llvm =
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e));
    report.query("local future actual LLVM verifier", &llvm);
    println!(
        "LOCAL FUTURE FINAL case={} unit={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("local future observed stage failures", report))
    }
}

#[test]
fn local_sources_follow_actual_future_uses() {
    let mut observations = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!(
                "LOCAL FUTURE CASE {} unit={unit} result={result:?}",
                case.label
            );
            observations.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(observations.len(), 108);
    assert!(
        observations.iter().all(|(_, _, result)| result.is_ok()),
        "{observations:#?}"
    );
}
