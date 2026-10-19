//! Every fixed A1 program executes its original entry once, including encoded true/false arms.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::scope_a1_sources,
};
fn run(unit: bool) {
    for c in scope_a1_sources() {
        let out = if unit {
            run_unit(c.label, c.source)
        } else {
            emit_link_and_run("provider.ko", c.source, "entry")
        };
        assert_result(
            c.label,
            if unit { "unit" } else { "single" },
            &out,
            c.golden,
        );
    }
}
#[test]
fn single_scope_a1_frozen_programs_keep_full_golden() {
    run(false);
}
#[test]
fn unit_scope_a1_frozen_programs_keep_full_golden() {
    run(true);
}
