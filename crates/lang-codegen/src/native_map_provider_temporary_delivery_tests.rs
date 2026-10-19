//! Actual ordinary delivery, deinit timing and named evaluation order through both entries.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::temporary_delivery_sources,
};

fn run(unit: bool) {
    for case in temporary_delivery_sources() {
        for control in [false, true] {
            let (source, golden) = if control {
                (&case.control, &case.control_golden)
            } else {
                (&case.source, &case.golden)
            };
            let label = format!("{}-control-{control}", case.label);
            let output = if unit {
                run_unit(&label, source)
            } else {
                emit_link_and_run("provider.ko", source, "entry")
            };
            assert_result(
                &label,
                if unit { "unit" } else { "single" },
                &output,
                golden,
            );
        }
    }
}

#[test]
fn single_ordinary_delivery_preserves_order_and_deinit() {
    run(false);
}

#[test]
fn unit_ordinary_delivery_preserves_order_and_deinit() {
    run(true);
}
