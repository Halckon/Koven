//! SPEC-0291: String value observation after nullable consume, ordinary entries only.
//! Collect both complete FE/ownership/SSA/LLVM outcomes before any assertion.

use super::map_consume_fixture_tests::{Fixture, observe_single_fixture, observe_unit_fixture};

mod inputs;

const FIXTURE: Fixture = Fixture {
    source_name: "nullable-map-consume-strings.ko",
    source: include_str!("../../tests/fixtures/map-consume-0291/nullable-strings.ko"),
    consumer: "package q\nfun entry(): Unit { p.main() }\n",
    consumes: 1,
};

#[test]
fn complete_nullable_map_consume_string_fixture_reaches_verified_llvm() {
    let mut observations = Vec::with_capacity(2);
    for unit in [false, true] {
        let mut stages = Vec::new();
        let outcome = if unit {
            observe_unit_fixture(&FIXTURE, inputs::inspect_unit, &mut stages)
        } else {
            observe_single_fixture(&FIXTURE, inputs::inspect_single, &mut stages)
        };
        let entry = if unit {
            "unit q.entry -> p.main"
        } else {
            "single main"
        };
        observations.push((entry, stages, outcome));
    }
    for (entry, stages, outcome) in &observations {
        println!(
            "NULLABLE MAP CONSUME STRING FIXTURE {entry}\n{}",
            stages.join("\n")
        );
        match outcome {
            Ok(()) => println!("outcome=verified LLVM"),
            Err(failure) => println!("failed stage={}; detail={}", failure.stage, failure.detail),
        }
    }
    assert_eq!(observations.len(), 2);
    assert!(
        observations.iter().all(|(_, _, outcome)| outcome.is_ok()),
        "both complete ordinary nullable String fixture entries must reach verified LLVM: {observations:#?}"
    );
}
