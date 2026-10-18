//! Original ordinary sequential Borrow sources; no instrumentation or replacement products.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::sequential_sources,
};

#[test]
fn single_sequential_borrowed_maps_exit_with_exact_cleanup_and_order() {
    for case in sequential_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}

#[test]
fn unit_sequential_borrowed_maps_exit_with_exact_cleanup_and_order() {
    for case in sequential_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
