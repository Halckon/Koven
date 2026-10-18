//! Ordinary source-form native checks, after both real SSA/LLVM entries succeed.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::source_form_sources,
};
#[test]
fn single_owned_mutable_and_group_sources_exit_normally() {
    for case in source_form_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_owned_mutable_and_group_sources_exit_normally() {
    for case in source_form_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
