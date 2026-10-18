//! Ordinary unmodified-source histories, paired with independently specified stdout.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::history_sources,
};

#[test]
fn single_storage_histories_exit_normally_in_insertion_order() {
    for case in history_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}

#[test]
fn unit_storage_histories_exit_normally_in_insertion_order() {
    for case in history_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
