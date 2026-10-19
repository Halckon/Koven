//! Ordinary native checks for represented Resource payloads, after the SSA/LLVM gate.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::resource_form_sources,
};
#[test]
fn single_existing_resource_representations_exit_normally() {
    for case in resource_form_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_existing_resource_representations_exit_normally() {
    for case in resource_form_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
