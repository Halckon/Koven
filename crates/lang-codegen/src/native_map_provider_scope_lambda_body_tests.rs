//! Original P15 entries execute once per pipeline with their complete frozen goldens.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::scope_a2_sources,
};
fn run(unit: bool) {
    for c in scope_a2_sources() {
        let output = if unit {
            run_unit(c.label, c.source)
        } else {
            emit_link_and_run("provider.ko", c.source, "entry")
        };
        assert_result(
            c.label,
            if unit { "unit" } else { "single" },
            &output,
            c.golden,
        );
    }
}
#[test]
fn single_scope_a2_lambda_bodies_keep_full_golden() {
    run(false);
}
#[test]
fn unit_scope_a2_lambda_bodies_keep_full_golden() {
    run(true);
}
