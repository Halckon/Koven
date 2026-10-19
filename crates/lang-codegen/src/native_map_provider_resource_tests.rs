//! Finite Resource Map source ownership; gated first by both real SSA/LLVM pipelines.
//! Ordinary outputs prove ordering, not dynamic release counts or absence of leaks.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::resource_sources,
};

#[test]
fn single_resource_map_sources_exit_normally() {
    for (case, source, golden) in resource_sources() {
        let run = emit_link_and_run("provider.ko", source, "entry");
        assert_result(case, "single", &run, golden);
    }
}

#[test]
fn unit_resource_map_sources_exit_normally() {
    for (case, source, golden) in resource_sources() {
        let run = run_unit(case, source);
        assert_result(case, "unit", &run, golden);
    }
}
