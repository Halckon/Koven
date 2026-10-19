//! Core and generic-wrapper native groups use their exact public pipeline inputs.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::{
        local_argument_sources, local_generic_argument_sources,
    },
};
fn run_core(unit: bool) {
    for c in local_argument_sources() {
        if c.unit_only && !unit {
            continue;
        }
        let output = if unit {
            run_unit(&c.label, &c.source)
        } else {
            emit_link_and_run("provider.ko", &c.source, "entry")
        };
        assert_result(
            &c.label,
            if unit { "unit" } else { "single" },
            &output,
            &c.golden,
        );
    }
}
fn run_wrapper(unit: bool) {
    for (label, source, golden) in local_generic_argument_sources() {
        let output = if unit {
            run_unit(&label, &source)
        } else {
            emit_link_and_run("provider.ko", &source, "entry")
        };
        assert_result(
            &label,
            if unit { "unit" } else { "single" },
            &output,
            &golden,
        );
    }
}
#[test]
fn single_local_argument_sources_keep_exact_native_order_and_cleanup() {
    run_core(false);
}
#[test]
fn unit_local_argument_sources_keep_exact_native_order_and_cleanup() {
    run_core(true);
}
#[test]
fn single_generic_wrapper_sources_keep_exact_native_order_and_cleanup() {
    run_wrapper(false);
}
#[test]
fn unit_generic_wrapper_sources_keep_exact_native_order_and_cleanup() {
    run_wrapper(true);
}
