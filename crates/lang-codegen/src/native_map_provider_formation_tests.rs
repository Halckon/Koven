//! Original legal formation/transfer sources; native output does not replace owner identity queries.
use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run,
    ssa::map_provider_exits_pipeline_tests::{
        alternate_local_sources, formation_runtime_sources, future_transfer_sources,
    },
};

fn cases() -> Vec<(String, String, &'static str)> {
    let mut cases = alternate_local_sources()
        .into_iter()
        .map(|(label, source)| {
            let golden = match label {
                "reassigned" | "parameter-move" => "gap\n",
                "old-alias-rebound" => "gap\nzero\n",
                "prefix-cfg" => "zero\ngap\n",
                "prefix-put" => "gap\nkey\nvalue\n",
                other => panic!("unexpected original formation case: {other}"),
            };
            (label.to_owned(), source, golden)
        })
        .collect::<Vec<_>>();
    cases.extend(
        future_transfer_sources()
            .into_iter()
            .map(|(mutable, source)| {
                (
                    format!(
                        "future-owner-transfer-{}",
                        if mutable { "MutableMap" } else { "Map" }
                    ),
                    source,
                    "sink\n",
                )
            }),
    );
    assert_eq!(cases.len(), 7);
    cases
}

#[test]
fn single_original_formations_and_owner_transfers_exit_normally() {
    for (label, source, golden) in cases() {
        let run = emit_link_and_run("provider.ko", &source, "entry");
        assert_result(&label, "single", &run, golden);
    }
}

#[test]
fn unit_original_formations_and_owner_transfers_exit_normally() {
    for (label, source, golden) in cases() {
        let run = run_unit(&label, &source);
        assert_result(&label, "unit", &run, golden);
    }
}

#[test]
fn single_resource_formations_and_owner_transfers_preserve_cleanup() {
    for case in formation_runtime_sources() {
        let run = emit_link_and_run("provider.ko", &case.source, "entry");
        assert_result(&case.label, "single", &run, &case.golden);
    }
}

#[test]
fn unit_resource_formations_and_owner_transfers_preserve_cleanup() {
    for case in formation_runtime_sources() {
        let run = run_unit(&case.label, &case.source);
        assert_result(&case.label, "unit", &run, &case.golden);
    }
}
