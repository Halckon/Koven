//! The same twelve G8 sources execute nonempty, independent empty-inner and empty-outer paths.
use super::{assert_result, run_unit};
use crate::ssa::map_provider_exits_pipeline_tests::stack_runtime_sources;

fn run(unit: bool) {
    let cases = stack_runtime_sources();
    assert_eq!(cases.len(), 12);
    for case in cases {
        let result = if unit {
            run_unit(&case.label, &case.source)
        } else {
            crate::native_tests::emit_link_and_run("provider.ko", &case.source, "entry")
        };
        assert_result(
            &case.label,
            if unit { "unit" } else { "single" },
            &result,
            &case.golden,
        );
    }
}

#[test]
fn single_nested_stack_paths_preserve_cleanup_and_caller_permissions() {
    run(false);
}

#[test]
fn unit_nested_stack_paths_preserve_cleanup_and_caller_permissions() {
    run(true);
}
