//! Strict native checks for Resource local future-use corners and real empty inputs.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::local_resource_future_sources,
};
#[test]
fn single_resource_future_locals_exit_normally() {
    for case in local_resource_future_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_resource_future_locals_exit_normally() {
    for case in local_resource_future_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
