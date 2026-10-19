//! Both runtime argument CFG arms execute the original empty temporary-source cases.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::temporary_argument_cfg_sources,
};

fn run(unit: bool) {
    for case in temporary_argument_cfg_sources() {
        let output = if unit {
            run_unit(&case.label, &case.source)
        } else {
            emit_link_and_run("provider.ko", &case.source, "entry")
        };
        assert_result(
            &case.label,
            if unit { "unit" } else { "single" },
            &output,
            &case.golden,
        );
    }
}

#[test]
fn single_argument_cfg_edges_exit_normally() {
    run(false);
}

#[test]
fn unit_argument_cfg_edges_exit_normally() {
    run(true);
}
