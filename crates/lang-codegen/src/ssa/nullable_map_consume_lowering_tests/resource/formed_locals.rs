//! Same-source Unit collector plus the independently reviewed formed-local exact contract.
//! All eighteen entries are collected before input/completeness or compatibility assertions.

use super::{LowerOutcome, Observation, Trace, print_outcome};

mod inputs;
mod query;
mod sources;
mod syntax;

fn collect(with_formed_queries: bool) -> Vec<Observation> {
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
                    |sources, file, names, typed, owned, trace| {
                        if with_formed_queries {
                            inputs::unit_formed(case, sources, file, names, typed, owned, trace);
                        }
                    },
                    0,
                    &mut trace,
                )
            } else {
                super::memory::observe_source(
                    case.path,
                    case.single,
                    |sources, file, names, typed, trace| {
                        inputs::memory(case, sources, file, names, typed, trace)
                    },
                    |sources, file, names, typed, owned, trace| {
                        inputs::memory_formed(case, sources, file, names, typed, owned, trace)
                    },
                    0,
                    |_, _, _, _, _| {},
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
    // No source is skipped or asserted before all eighteen observations are printed.
    assert_eq!(observations.len(), 18);
    let labels: Vec<_> = observations
        .iter()
        .map(|observation| observation.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "P1/single",
            "P1/unit",
            "P2/single",
            "P2/unit",
            "P3/single",
            "P3/unit",
            "K1/single",
            "K1/unit",
            "K2/single",
            "K2/unit",
            "K3/single",
            "K3/unit",
            "K4/single",
            "K4/unit",
            "K5/single",
            "K5/unit",
            "K6/single",
            "K6/unit",
        ]
    );
    assert!(
        observations
            .iter()
            .all(|observation| observation.outcome.is_ok()
                && observation.trace.input_failures.is_empty()),
        "all eighteen source/FE/input/ownership gates must pass and actual lowering must be attempted"
    );
    observations
}

#[test]
fn nullable_map_formed_local_sources_collect_baseline_outcomes() {
    let observations = collect(false);
    // Historical collection-only contract: non-K1 lower outcomes remain observations.
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
        "COLLECTION ONLY: all eighteen ordinary entries attempted lowering; K1 retains two LLVM successes. Other outcomes are observations only; original MemoryDocument facts are checked without freezing other lower outcomes."
    );
}
