//! Real ordinary return operands, empty inputs and lexical Guard cleanup.
use super::{assert_result, run_unit};
use crate::ssa::map_provider_exits_pipeline_tests::{
    ordinary_control_flow_neighbor_sources, ordinary_control_flow_sources, ordinary_return_sources,
};

fn run(unit: bool) {
    for case in ordinary_return_sources() {
        let run = if unit {
            run_unit(&case.label, &case.source)
        } else {
            crate::native_tests::emit_link_and_run("provider.ko", &case.source, "entry")
        };
        assert_result(
            &case.label,
            if unit { "unit" } else { "single" },
            &run,
            &case.golden,
        );
    }
}

#[test]
fn single_ordinary_returns_preserve_delivery_and_cleanup() {
    run(false);
}

#[test]
fn unit_ordinary_returns_preserve_delivery_and_cleanup() {
    run(true);
}

fn run_cfg(unit: bool, neighbors: bool) {
    let cases = if neighbors {
        ordinary_control_flow_neighbor_sources()
    } else {
        ordinary_control_flow_sources()
    };
    assert_eq!(cases.len(), if neighbors { 14 } else { 24 });
    for case in cases {
        let result = if unit {
            run_unit(&case.label, &case.source)
        } else {
            crate::native_tests::emit_link_and_run("provider.ko", &case.source, "entry")
        };
        assert_result(
            &case.label,
            if unit { "unit" } else { "single" },
            &result,
            &case.golden,
        );
    }
}

#[test]
fn single_ordinary_cfg_preserves_branches_and_cleanup() {
    run_cfg(false, false);
}

#[test]
fn unit_ordinary_cfg_preserves_branches_and_cleanup() {
    run_cfg(true, false);
}

#[test]
fn single_cfg_neighbors_preserve_current_provider_and_cleanup() {
    run_cfg(false, true);
}

#[test]
fn unit_cfg_neighbors_preserve_current_provider_and_cleanup() {
    run_cfg(true, true);
}
