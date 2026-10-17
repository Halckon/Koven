//! Ordinary baseline collector for composable local-storage planning, before a new FE fact.
//! All eighteen entries are collected before input/completeness or compatibility assertions.

use super::{LowerOutcome, Observation, Trace, print_outcome};

mod inputs;
mod sources;
mod syntax;

#[test]
fn nullable_map_formed_local_sources_collect_baseline_outcomes() {
    let mut observations = Vec::new();
    for case in sources::CASES {
        for unit in [false, true] {
            let mut trace = Trace::default();
            let outcome = if unit {
                super::unit::observe_sources(
                    case.provider,
                    case.consumer,
                    |sources, file, names, typed, trace| {
                        inputs::unit(case, sources, file, names, typed, trace)
                    },
                    0,
                    &mut trace,
                )
            } else {
                super::single::observe_source(
                    case.path,
                    case.single,
                    |sources, parsed, names, typed, trace| {
                        inputs::single(case, sources, parsed, names, typed, trace)
                    },
                    0,
                    &mut trace,
                )
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
            "FORMATION BASELINE {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        println!("input failures={:?}", observation.trace.input_failures);
        if let Ok(observed) = &observation.outcome {
            println!("actual selected entry={:#?}", observed.entry);
        }
        print_outcome(&observation.outcome);
    }
    // No source is skipped or asserted early; non-K1 lower outcomes are observations only.
    assert_eq!(observations.len(), 18);
    assert!(
        observations
            .iter()
            .all(|observation| observation.outcome.is_ok()
                && observation.trace.input_failures.is_empty()),
        "all eighteen source/FE/input/ownership gates must pass and actual lowering must be attempted"
    );
    let frozen: Vec<_> = observations
        .iter()
        .filter(|observation| matches!(observation.label.as_str(), "K1/single" | "K1/unit"))
        .collect();
    assert_eq!(frozen.len(), 2);
    assert!(
        frozen.iter().all(
            |observation| observation.outcome.as_ref().is_ok_and(|actual| {
                let (name, path) = if observation.label == "K1/single" {
                    ("main", "unreachable_nullable_map.ko")
                } else {
                    ("entry", "q/consumer.ko")
                };
                actual.entry.name == name
                    && actual
                        .entry
                        .path
                        .as_ref()
                        .is_ok_and(|actual| actual == path)
                    && matches!(&actual.outcome, LowerOutcome::VerifiedLlvm)
            })
        ),
        "the previously frozen K1 dead-tail LLVM successes must be preserved"
    );
    println!(
        "COLLECTION ONLY: all eighteen ordinary entries attempted lowering; K1 retains two LLVM successes. Other outcomes await review; no formed-local API exists in this test slice."
    );
}
