//! Resource observations plus the reviewed Resource compatibility expectations.
//! The collector alone remains insufficient for a compatibility verdict.
//! Resource source wholes include real assertion Abort edges and remain CI-only.

use super::{
    Failure, FunctionId, Outcome, ParsedFile, Program, SourceMap, Span, Trace, admission_error,
    diagnostic_text, diagnostics, gate, origin, parsed, step,
};
use crate::ssa::{LoweringError, LoweringErrorKind};
use sources::{CASES, Case};

mod contract;
mod formed_locals;
mod memory;
mod runtime;
mod single;
mod sources;
mod unit;
mod unreachable_storage;

/// Path, single-file text, provider text and consumer text from the existing R1 case.
pub(super) fn registration_r1_source() -> (&'static str, &'static str, &'static str, &'static str) {
    let case = CASES
        .iter()
        .find(|case| case.id == "R1")
        .expect("frozen R1");
    assert!(case.has_ordinary);
    assert_eq!(case.counts, [2, 2, 1, 1, 0, 1]);
    (case.path, case.single, case.provider, case.consumer)
}

#[derive(Debug)]
enum LowerOutcome {
    VerifiedLlvm,
    LowerRejected {
        error: LoweringError,
        origin: Option<String>,
        location: Option<ErrorLocation>,
    },
    VerifierRejected {
        detail: String,
    },
    LlvmRejected {
        detail: String,
    },
}

#[derive(Debug)]
struct ErrorLocation {
    path: Result<String, String>,
    text: Result<String, String>,
}

#[derive(Debug)]
struct SelectedEntry {
    name: String,
    path: Result<String, String>,
    span: Span,
}

fn selected_entry(sources: &SourceMap, name: &str, span: Span) -> SelectedEntry {
    SelectedEntry {
        name: name.to_owned(),
        path: sources
            .source_name(span.source_id())
            .map(str::to_owned)
            .map_err(|error| format!("{error:#?}")),
        span,
    }
}

#[derive(Debug)]
struct LowerObservation {
    // Captured from the same real declaration used as the explicit lowering entry.
    entry: SelectedEntry,
    outcome: LowerOutcome,
}

#[derive(Debug)]
struct Observation {
    label: String,
    trace: Trace,
    // Err is a prerequisite failure. Ok records one of four actual lower outcomes.
    outcome: Outcome<LowerObservation>,
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
                location: error.span.map(|span| ErrorLocation {
                    path: sources
                        .source_name(span.source_id())
                        .map(str::to_owned)
                        .map_err(|error| format!("{error:#?}")),
                    text: sources
                        .slice(span)
                        .map(str::to_owned)
                        .map_err(|error| format!("{error:#?}")),
                }),
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

fn print_outcome(outcome: &Outcome<LowerObservation>) {
    let outcome = match outcome {
        Ok(observation) => &observation.outcome,
        Err(failure) => {
            println!(
                "prerequisite failed stage={}; detail={}",
                failure.stage, failure.detail,
            );
            return;
        }
    };
    match outcome {
        LowerOutcome::VerifiedLlvm => println!(
            "observed outcome=VerifiedLlvm; per-source compatibility verdict still pending review"
        ),
        LowerOutcome::LowerRejected { error, origin, .. } => {
            let category = if error.kind == LoweringErrorKind::UnsupportedNode {
                "unreviewed capability rejection"
            } else {
                "unresolved lowering error (including MissingFact/InvalidModel)"
            };
            println!(
                "observed outcome=LowerRejected; category={category}; error={error:#?}; origin={origin:?}"
            );
        }
        LowerOutcome::VerifierRejected { detail } => println!(
            "observed outcome=VerifierRejected; unresolved verifier defect; detail={detail}"
        ),
        LowerOutcome::LlvmRejected { detail } => {
            println!("observed outcome=LlvmRejected; unresolved LLVM defect; detail={detail}")
        }
    }
}

fn collect_observations(strong_contract: bool) -> Vec<Observation> {
    let mut observations = Vec::with_capacity(8);
    for case in CASES {
        for unit in [false, true] {
            let mut trace = Trace::default();
            let outcome = if unit {
                unit::observe(case, strong_contract, &mut trace)
            } else {
                memory::observe(case, strong_contract, &mut trace)
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
    observations
}

#[test]
fn nullable_map_resource_sources_collect_baseline_outcomes() {
    let _ = collect_observations(false);
    println!(
        "COLLECTION ONLY: FE/ownership/input gates passed; eight lower outcomes recorded. \
         No per-source success or rejection contract has been accepted by this collector."
    );
}

#[derive(Debug)]
struct Baseline {
    label: &'static str,
    entry_name: &'static str,
    entry_path: &'static str,
}

// Both original MemoryDocument and rooted p/q layouts now require verified LLVM.
// Historical exact p/q rejections are retained outside this active contract.
// Never allow wildcard errors, ignored rows, or skipped entries to hide a regression.
const RESOURCE_COMPATIBILITY: &[Baseline] = &[
    Baseline {
        label: "R0/single",
        entry_name: "main",
        entry_path: "r0_remove_only.ko",
    },
    Baseline {
        label: "R0/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
    Baseline {
        label: "R1/single",
        entry_name: "main",
        entry_path: "r1_ordinary_source_before_remove.ko",
    },
    Baseline {
        label: "R1/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
    Baseline {
        label: "R2/single",
        entry_name: "main",
        entry_path: "r2_ordinary_source_after_remove.ko",
    },
    Baseline {
        label: "R2/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
    Baseline {
        label: "R3/single",
        entry_name: "main",
        entry_path: "r3_ordinary_null_only.ko",
    },
    Baseline {
        label: "R3/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
    },
];

#[test]
fn nullable_map_resource_sources_preserve_compatibility() {
    let observations = collect_observations(true);
    assert_eq!(observations.len(), RESOURCE_COMPATIBILITY.len());
    let mut mismatches = Vec::new();
    for (actual, expected) in observations.iter().zip(RESOURCE_COMPATIBILITY) {
        let matched = actual.label == expected.label
            && actual.outcome.as_ref().is_ok_and(|observed| {
                observed.entry.name == expected.entry_name
                    && observed
                        .entry
                        .path
                        .as_ref()
                        .is_ok_and(|path| path.as_str() == expected.entry_path)
                    && !observed.entry.span.is_empty()
                    && matches!(&observed.outcome, LowerOutcome::VerifiedLlvm)
            });
        println!(
            "RESOURCE COMPATIBILITY {}: matched={matched}",
            expected.label
        );
        if !matched {
            mismatches.push(format!("expected={expected:#?}; actual={actual:#?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "exact Resource compatibility mismatches:\n{}",
        mismatches.join("\n")
    );
}
