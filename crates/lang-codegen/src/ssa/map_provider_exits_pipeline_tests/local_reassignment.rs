//! Legal source replacement histories: ordinary FE/lowering observations precede repairs.
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
        "LOCAL REASSIGN INPUT case={} unit={unit} source={:?} golden={:?}",
        case.label, case.source, case.golden
    );
    println!(
        "LOCAL REASSIGN AXES mutable={} payload={} history={}",
        case.mutable, case.payload, case.history
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
    let expected = if case.history == 1 { 4 } else { 2 };
    // Public identity queries are recorded before lowering and never short-circuit
    // that lowering. Failed compilation remains the actual observed error.
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                report.check(
                    "reassignment original qualified descriptor count",
                    typed.map_iterations().len() == expected,
                    typed.map_iterations(),
                );
                println!(
                    "LOCAL REASSIGN UNIT FACTS case={} formed={:#?} plans={:#?} drops={:#?} deliveries={:#?} loans={:#?}",
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
                report.check(
                    "reassignment original descriptor count",
                    typed.map_iterations().len() == expected,
                    typed.map_iterations(),
                );
                println!(
                    "LOCAL REASSIGN SINGLE FACTS case={} formed={:#?} plans={:#?} drops={:#?} conditions={:#?} bindings={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.cleanup_conditions(),
                    owned.bindings(),
                    owned.loans()
                );
            },
        )
    };
    println!(
        "LOCAL REASSIGN LOWER case={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    let verified =
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e));
    report.query("reassignment actual SSA verifier", &verified);
    let llvm =
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e));
    report.query("reassignment actual LLVM verifier", &llvm);
    println!(
        "LOCAL REASSIGN FINAL case={} unit={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("reassignment observed stage failures", report))
    }
}

#[test]
fn reassigned_sources_preserve_actual_value_histories() {
    let mut observations = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!(
                "LOCAL REASSIGN CASE {} unit={unit} result={result:?}",
                case.label
            );
            observations.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(observations.len(), 36);
    assert!(
        observations.iter().all(|(_, _, result)| result.is_ok()),
        "{observations:#?}"
    );
}
