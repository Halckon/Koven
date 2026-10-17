//! Resource baseline collection only. A passing collector is not a compatibility verdict.
//! Every per-source/entry lowering verdict must be reviewed and frozen before production edits.

use super::{
    Failure, FunctionId, Outcome, ParsedFile, Program, SourceMap, Trace, admission_error,
    diagnostic_text, diagnostics, gate, origin, parsed, step,
};
use crate::ssa::{LoweringError, LoweringErrorKind};
use sources::{CASES, Case};

mod single;
mod sources;
mod unit;

#[derive(Debug)]
enum LowerOutcome {
    VerifiedLlvm,
    LowerRejected {
        error: LoweringError,
        origin: Option<String>,
    },
    VerifierRejected {
        detail: String,
    },
    LlvmRejected {
        detail: String,
    },
}

struct Observation {
    label: String,
    trace: Trace,
    // Err is a prerequisite failure. Ok records one of four actual lower outcomes.
    outcome: Outcome<LowerOutcome>,
}

fn finish_lower(
    sources: &SourceMap,
    result: Result<(Program, FunctionId), LoweringError>,
    trace: &mut Trace,
) -> LowerOutcome {
    let (program, entry) = match result {
        Ok(produced) => produced,
        Err(error) => {
            return LowerOutcome::LowerRejected {
                origin: error.span.map(|span| origin(sources, span)),
                error,
            };
        }
    };
    trace.stages.push("lower SSA: reached".to_owned());
    trace.stages.push(format!(
        "actual selected SSA entry={entry:?}; actual modules={:#?}",
        program.modules,
    ));
    if let Err(error) = crate::ssa::verify::verify_program(&program) {
        return LowerOutcome::VerifierRejected {
            detail: format!("{error:#?}"),
        };
    }
    trace.stages.push("verify SSA: reached".to_owned());
    trace.stages.push(format!(
        "verified SSA:\n{}",
        crate::ssa::render_program(&program)
    ));
    match crate::llvm::render_verified_program_with_entry(&program, entry) {
        Ok(llvm) => {
            trace
                .stages
                .push(format!("actual LLVM bytes={}; LLVM:\n{llvm}", llvm.len()));
            LowerOutcome::VerifiedLlvm
        }
        Err(error) => LowerOutcome::LlvmRejected {
            detail: format!("{error:#?}"),
        },
    }
}

fn print_outcome(outcome: &Outcome<LowerOutcome>) {
    match outcome {
        Err(failure) => println!(
            "prerequisite failed stage={}; detail={}",
            failure.stage, failure.detail,
        ),
        Ok(LowerOutcome::VerifiedLlvm) => println!(
            "observed outcome=VerifiedLlvm; per-source compatibility verdict still pending review"
        ),
        Ok(LowerOutcome::LowerRejected { error, origin }) => {
            let category = if error.kind == LoweringErrorKind::UnsupportedNode {
                "unreviewed capability rejection"
            } else {
                "unresolved lowering error (including MissingFact/InvalidModel)"
            };
            println!(
                "observed outcome=LowerRejected; category={category}; error={error:#?}; origin={origin:?}"
            );
        }
        Ok(LowerOutcome::VerifierRejected { detail }) => println!(
            "observed outcome=VerifierRejected; unresolved verifier defect; detail={detail}"
        ),
        Ok(LowerOutcome::LlvmRejected { detail }) => {
            println!("observed outcome=LlvmRejected; unresolved LLVM defect; detail={detail}")
        }
    }
}

#[test]
fn nullable_map_resource_sources_collect_baseline_outcomes() {
    let mut observations = Vec::with_capacity(8);
    for case in CASES {
        for unit in [false, true] {
            let mut trace = Trace::default();
            let outcome = if unit {
                unit::observe(case, &mut trace)
            } else {
                single::observe(case, &mut trace)
            };
            observations.push(Observation {
                label: format!("{}/{}", case.id, if unit { "unit" } else { "single" }),
                trace,
                outcome,
            });
        }
    }
    for observation in &observations {
        println!(
            "RESOURCE BASELINE {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        println!("input failures={:?}", observation.trace.input_failures);
        print_outcome(&observation.outcome);
    }
    // No per-source behavior assertion precedes collection of all eight ordinary entries.
    assert_eq!(observations.len(), 8);
    assert!(
        observations.iter().all(|observation| {
            observation.outcome.is_ok() && observation.trace.input_failures.is_empty()
        }),
        "all eight sources must pass input/FE/ownership gates and attempt real lowering"
    );
    println!(
        "COLLECTION ONLY: FE/ownership/input gates passed; eight lower outcomes recorded. \
         No per-source success or rejection contract has been accepted by this collector."
    );
}
