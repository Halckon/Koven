//! Exact fixture bytes and full golden output for intrinsic/Unit FunctionValue births.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::{MapBirthKind, map_birth_sources},
};
fn run(unit: bool) {
    for c in map_birth_sources() {
        if !unit && c.kind == MapBirthKind::FunctionValue {
            continue;
        }
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
fn single_intrinsic_births_keep_full_native_golden() {
    run(false);
}
#[test]
fn unit_intrinsic_and_function_value_births_keep_full_native_golden() {
    run(true);
}
