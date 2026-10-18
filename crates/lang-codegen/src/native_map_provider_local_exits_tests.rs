//! Ordinary native checks for real local formation and moved-alias exits.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::local_exits_sources,
};

#[test]
fn single_local_formation_exits_normally() {
    for case in local_exits_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}

#[test]
fn unit_local_formation_exits_normally() {
    for case in local_exits_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}

#[test]
fn unit_rebound_original_root_keeps_both_owners_separate() {
    let (label, source, golden) =
        crate::ssa::map_provider_exits_pipeline_tests::local_alias_rebound_source();
    let run = run_unit(label, &source);
    assert_result(label, "unit", &run, golden);
}
