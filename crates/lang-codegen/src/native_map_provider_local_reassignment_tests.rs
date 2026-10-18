//! Normal reassignment products; exact exit/stdout/stderr contracts use real source.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::local_reassignment_sources,
};

#[test]
fn single_local_reassignments_preserve_value_histories() {
    for case in local_reassignment_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}

#[test]
fn unit_local_reassignments_preserve_value_histories() {
    for case in local_reassignment_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}

#[test]
fn single_grouped_replacements_preserve_value_histories() {
    let cases = crate::ssa::map_provider_exits_pipeline_tests::local_grouped_reassignment_sources();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_grouped_replacements_preserve_value_histories() {
    let cases = crate::ssa::map_provider_exits_pipeline_tests::local_grouped_reassignment_sources();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}

#[test]
fn single_ordinary_epoch_arguments_preserve_replacement_order() {
    for (label, source, golden) in
        crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_epoch_sources()
    {
        let run = emit_link_and_run("provider.ko", &source, "entry");
        assert_result(label, "single", &run, golden);
    }
}
#[test]
fn unit_ordinary_epoch_arguments_preserve_replacement_order() {
    for (label, source, golden) in
        crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_epoch_sources()
    {
        let run = run_unit(label, &source);
        assert_result(label, "unit", &run, golden);
    }
}

#[test]
fn unit_ordinary_epoch_routes_preserve_replacement_order() {
    let cases = crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_route_sources();
    assert_eq!(cases.len(), 6);
    for (label, source, golden) in cases {
        let run = run_unit(&label, &source);
        assert_result(&label, "unit", &run, &golden);
    }
}
#[test]
fn unit_ordinary_route_controls_preserve_existing_delivery() {
    let cases = crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_route_controls();
    assert_eq!(cases.len(), 3);
    for (label, source, golden) in cases {
        let run = run_unit(&label, &source);
        assert_result(&label, "unit", &run, &golden);
    }
}

#[test]
fn single_completed_replacements_drop_the_installed_owner_asap() {
    let cases = crate::ssa::map_provider_exits_pipeline_tests::local_completed_epoch_sources();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}
#[test]
fn unit_completed_replacements_drop_the_installed_owner_asap() {
    let cases = crate::ssa::map_provider_exits_pipeline_tests::local_completed_epoch_sources();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
