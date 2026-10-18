//! Four newly rebuilt Borrow Map sources, accepted only after their eight real pipelines pass.
//! Ordinary emit/link/run only; no injected failure, instrumentation or sanitizer controls.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::borrowed_exit_sources,
};

#[test]
fn single_borrowed_map_exit_sources_exit_normally() {
    for (case, source, golden) in borrowed_exit_sources() {
        let run = emit_link_and_run("provider.ko", source, "entry");
        assert_result(case, "single", &run, golden);
    }
}

#[test]
fn unit_borrowed_map_exit_sources_exit_normally() {
    for (case, source, golden) in borrowed_exit_sources() {
        let run = run_unit(case, source);
        assert_result(case, "unit", &run, golden);
    }
}
