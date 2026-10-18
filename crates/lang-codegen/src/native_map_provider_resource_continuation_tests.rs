//! Strict normal-source Resource continuations, using the independently gated source/golden set.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::resource_continuation_sources,
};
#[test]
fn single_resource_continuations_exit_with_lexical_drops() {
    for case in resource_continuation_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_resource_continuations_exit_with_lexical_drops() {
    for case in resource_continuation_sources() {
        let run = run_unit(case.label, &case.source);
        assert_result(case.label, "unit", &run, &case.golden);
    }
}
