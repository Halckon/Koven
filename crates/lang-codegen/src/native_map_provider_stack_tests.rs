//! The same G8 pipeline sources execute their nonempty and independently empty paths.
use super::{assert_result, run_unit};
use crate::ssa::map_provider_exits_pipeline_tests::{
    StackCase, stack_owned_resource_sources, stack_runtime_sources, stack_variant_sources,
};

fn run(unit: bool, cases: Vec<StackCase>, count: usize) {
    assert_eq!(cases.len(), count);
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
    run(false, stack_runtime_sources(), 12);
}

#[test]
fn unit_nested_stack_paths_preserve_cleanup_and_caller_permissions() {
    run(true, stack_runtime_sources(), 12);
}

#[test]
fn single_owned_resource_same_root_preserves_exact_cleanup() {
    run(false, stack_owned_resource_sources(), 4);
}

#[test]
fn unit_owned_resource_same_root_preserves_exact_cleanup() {
    run(true, stack_owned_resource_sources(), 4);
}

#[test]
fn single_temporary_nullable_and_three_layers_preserve_each_exit() {
    run(false, stack_variant_sources(), 18);
}

#[test]
fn unit_temporary_nullable_and_three_layers_preserve_each_exit() {
    run(true, stack_variant_sources(), 18);
}
