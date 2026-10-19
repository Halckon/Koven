//! Ordinary Pure future-use native checks after the real two-entry pipeline succeeds.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::future_use_sources,
};
#[test]
fn single_pure_owned_future_uses_exit_normally() {
    for case in future_use_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_pure_owned_future_uses_exit_normally() {
    for case in future_use_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
