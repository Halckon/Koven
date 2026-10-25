//! Exact ordinary formed-local demands and reviewed lower outcomes, separate from the collector.

use super::super::{LowerOutcome, LoweringErrorKind};

#[derive(Debug)]
enum ExpectedLower {
    VerifiedLlvm,
    Rejected {
        kind: LoweringErrorKind,
        path: &'static str,
        start: usize,
        end: usize,
        text: &'static str,
    },
}

#[derive(Debug)]
struct Expected {
    label: &'static str,
    entry_name: &'static str,
    entry_path: &'static str,
    lower: ExpectedLower,
}

// Fourteen LLVM successes plus exact K3/K4 capability rejections in both Unit layouts.
// K4 retains each original null origin; the historical p/q MissingFact is not a contract.
// Each row is independent of observed outcomes; no wildcard failure counts as preservation.
const EXPECTED: &[Expected] = &[
    Expected {
        label: "P1/single",
        entry_name: "main",
        entry_path: "formed_p1_prefix_present.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "P1/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "P2/single",
        entry_name: "main",
        entry_path: "formed_p2_seed_then_move.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "P2/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "P3/single",
        entry_name: "main",
        entry_path: "formed_p3_branch_present.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "P3/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K1/single",
        entry_name: "main",
        entry_path: "unreachable_nullable_map.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K1/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K2/single",
        entry_name: "main",
        entry_path: "formed_k2_initializer_return.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K2/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K3/single",
        entry_name: "main",
        entry_path: "formed_k3_constant_false_unsupported.ko",
        lower: ExpectedLower::Rejected {
            kind: LoweringErrorKind::UnsupportedNode,
            path: "formed_k3_constant_false_unsupported.ko",
            start: 42,
            end: 48,
            text: "branch",
        },
    },
    Expected {
        label: "K3/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::Rejected {
            kind: LoweringErrorKind::UnsupportedNode,
            path: "p/provider.ko",
            start: 52,
            end: 58,
            text: "branch",
        },
    },
    Expected {
        label: "K4/single",
        entry_name: "main",
        entry_path: "formed_k4_lambda_scope.ko",
        lower: ExpectedLower::Rejected {
            kind: LoweringErrorKind::UnsupportedNode,
            path: "formed_k4_lambda_scope.ko",
            start: 88,
            end: 92,
            text: "null",
        },
    },
    Expected {
        label: "K4/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::Rejected {
            kind: LoweringErrorKind::UnsupportedNode,
            path: "p/provider.ko",
            start: 98,
            end: 102,
            text: "null",
        },
    },
    Expected {
        label: "K5/single",
        entry_name: "main",
        entry_path: "formed_k5_shadow_dead_tail.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K5/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K6/single",
        entry_name: "main",
        entry_path: "formed_k6_loop_existing_header.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
    Expected {
        label: "K6/unit",
        entry_name: "entry",
        entry_path: "q/consumer.ko",
        lower: ExpectedLower::VerifiedLlvm,
    },
];

fn matches_lower(expected: &ExpectedLower, actual: &LowerOutcome) -> bool {
    match (expected, actual) {
        (ExpectedLower::VerifiedLlvm, LowerOutcome::VerifiedLlvm) => true,
        (
            ExpectedLower::Rejected {
                kind,
                path,
                start,
                end,
                text,
            },
            LowerOutcome::LowerRejected {
                error,
                location: Some(location),
                ..
            },
        ) => {
            error.kind == *kind
                && error
                    .span
                    .is_some_and(|span| span.start() == *start && span.end() == *end)
                && location
                    .path
                    .as_ref()
                    .is_ok_and(|actual| actual.as_str() == *path)
                && location
                    .text
                    .as_ref()
                    .is_ok_and(|actual| actual.as_str() == *text)
        }
        // Prerequisite, verifier, LLVM, missing-span and wrong-origin failures do not match.
        _ => false,
    }
}

#[test]
fn nullable_map_formed_local_demands_preserve_and_advance_exact_outcomes() {
    let observations = super::collect(true);
    assert_eq!(observations.len(), EXPECTED.len());
    let mut mismatches = Vec::new();
    for (actual, expected) in observations.iter().zip(EXPECTED) {
        let matched = actual.label == expected.label
            && actual.outcome.as_ref().is_ok_and(|observed| {
                observed.entry.name == expected.entry_name
                    && observed
                        .entry
                        .path
                        .as_ref()
                        .is_ok_and(|path| path.as_str() == expected.entry_path)
                    && !observed.entry.span.is_empty()
                    && matches_lower(&expected.lower, &observed.outcome)
            });
        println!("FORMATION EXACT {}: matched={matched}", expected.label);
        if !matched {
            mismatches.push(format!("expected={expected:#?}; actual={actual:#?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "exact formed-local lower outcome mismatches:\n{}",
        mismatches.join("\n")
    );
}
