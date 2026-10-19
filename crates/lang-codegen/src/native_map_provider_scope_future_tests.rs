//! All six original D sources retain their frozen full stdout under both entries.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::scope_future_sources,
};
#[test]
fn single_original_future_handoffs_keep_full_goldens() {
    let cases = scope_future_sources();
    assert_eq!(cases.len(), 6);
    for c in cases {
        let output = emit_link_and_run("provider.ko", c.source, "entry");
        assert_result(c.label, "single", &output, c.golden);
    }
}
#[test]
fn unit_original_future_handoffs_keep_full_goldens() {
    let cases = scope_future_sources();
    assert_eq!(cases.len(), 6);
    for c in cases {
        let output = run_unit(c.label, c.source);
        assert_result(c.label, "unit", &output, c.golden);
    }
}
