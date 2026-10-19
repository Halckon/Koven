//! The two unchanged P24 Unit entries retain their complete original goldens.
use super::{assert_result, run_unit};
use crate::ssa::map_provider_exits_pipeline_tests::scope_member_parameter_sources;
fn run(label: &str) {
    let c = scope_member_parameter_sources()
        .into_iter()
        .find(|c| c.label == label)
        .expect("one original P24 source");
    let output = run_unit(c.label, c.source);
    assert_result(c.label, "unit", &output, c.golden);
}
#[test]
fn unit_original_member_parameter_for_keeps_full_golden() {
    run("P24-for");
}
#[test]
fn unit_original_member_parameter_control_keeps_full_golden() {
    run("P24-control");
}
