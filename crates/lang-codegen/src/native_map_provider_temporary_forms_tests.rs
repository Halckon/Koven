//! Real represented temporary source programs, after both SSA/LLVM entries pass.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::temporary_form_sources,
};

#[test]
fn single_represented_temporary_sources_exit_normally() {
    for case in temporary_form_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}

#[test]
fn unit_represented_temporary_sources_exit_normally() {
    for case in temporary_form_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
