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

#[test]
fn single_resource_continue_exits_with_lexical_drops() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_continue_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_resource_continue_exits_with_lexical_drops() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_continue_sources() {
        let run = run_unit(case.label, &case.source);
        assert_result(case.label, "unit", &run, &case.golden);
    }
}

#[test]
fn single_nullable_resource_components_exit_with_exact_presence_and_drops() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_nullable_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_nullable_resource_components_exit_with_exact_presence_and_drops() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_nullable_sources() {
        let run = run_unit(case.label, &case.source);
        assert_result(case.label, "unit", &run, &case.golden);
    }
}

#[test]
fn single_resource_break_exits_with_lexical_drops() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_break_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_resource_break_exits_with_lexical_drops() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_break_sources() {
        let run = run_unit(case.label, &case.source);
        assert_result(case.label, "unit", &run, &case.golden);
    }
}

#[test]
fn single_resource_return_delivers_before_complete_cleanup() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_return_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_resource_return_delivers_before_complete_cleanup() {
    for case in crate::ssa::map_provider_exits_pipeline_tests::resource_return_sources() {
        let run = run_unit(case.label, &case.source);
        assert_result(case.label, "unit", &run, &case.golden);
    }
}
