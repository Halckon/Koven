//! Counter boundary consumption beyond the scalar/body audit.
use super::{UnitSource, source};

#[test]
fn counter_tail_complete_boundary_consumes_live_suffix() {
    for live in [0, 1, 2] {
        for rounds in [0, 2] {
            let text = source(2, true, live)
                .replace(
                    "while(flag)",
                    &format!("var remaining = {rounds}; var again = remaining > 0; while(again)"),
                )
                .replace(
                    " };",
                    " remaining = remaining - 1; again = remaining > 0; };",
                );
            let source = UnitSource::analyze("counter-boundary.ko", &text).unwrap();
            source.assert_frontend_clean();
            let (lowered, actions) =
                crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
            let (program, entry) = lowered.unwrap();
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "counter-entry-permit-consumed")
                    .count(),
                2
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "counter-replace-permit-consumed")
                    .count(),
                2
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "counter-tail-complete-boundary-audit")
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "counter-exit-binding-permit-consumed")
                    .count(),
                live
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "counter-exit-drop-permit-consumed")
                    .count(),
                2
            );
            for kind in [
                "counter-binding-permit-consumed",
                "counter-drop-permit-consumed",
            ] {
                assert_eq!(
                    actions.iter().filter(|(action, _)| *action == kind).count(),
                    2
                );
            }
            crate::ssa::verify::verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            super::super::assert_native_output(&source, b"");
        }
    }
}

#[test]
fn ci_only_counter_boundary_rejects_missing_suffix_and_wrong_entry() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let text = source(2, true, 2)
        .replace(
            "while(flag)",
            "var remaining = 2; var again = remaining > 0; while(again)",
        )
        .replace(
            " };",
            " remaining = remaining - 1; again = remaining > 0; };",
        );
    let source = UnitSource::analyze("counter-boundary-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::MissingCounterEntryPermit,
        OwnerFault::CounterEntryWrongSource,
        OwnerFault::CounterEntryDuplicateCommit,
        OwnerFault::MissingCounterExitBindingPermit,
        OwnerFault::MissingCounterExitDropPermit,
        OwnerFault::CounterExitWrongSource,
        OwnerFault::CounterExitDuplicateDrop,
        OwnerFault::MissingCounterBindingPermit,
        OwnerFault::MissingCounterDropPermit,
        OwnerFault::CounterBindingWrongSource,
        OwnerFault::CounterDropDuplicateCommit,
        OwnerFault::MissingCounterReplacePermit,
        OwnerFault::CounterReplaceWrongRoot,
        OwnerFault::CounterReplaceDuplicateCommit,
        OwnerFault::CounterLateAbortToHeader,
        OwnerFault::CounterBoundaryMissingSuffix,
        OwnerFault::CounterBoundaryWrongEntry,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| !kind.starts_with("counter-tail-"))
        );
    }
}

#[test]
fn counter_tail_partial_suffix_does_not_gain_complete_receipt() {
    let text = source(2, true, 0)
        .replace(
            "while(flag)",
            "var remaining = 0; var again = remaining > 0; while(again)",
        )
        .replace(
            " };",
            " remaining = remaining - 1; again = remaining > 0; }; println(r0); println(r1);",
        );
    let source = UnitSource::analyze("counter-partial-suffix.ko", &text).unwrap();
    source.assert_frontend_clean();
    let audit = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.counter_tail_evidence_v1())
        .unwrap();
    assert!(!audit.false_exit_partition().unwrap().0.is_empty());
    assert!(audit.false_exit_suffix().is_none());
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert!(
        actions
            .iter()
            .any(|(kind, _)| *kind == "counter-tail-owner-audit")
    );
    assert!(
        actions
            .iter()
            .all(|(kind, _)| *kind != "counter-tail-complete-boundary-audit")
    );
    super::super::assert_native_output(&source, b"entry0\nentry1\n");
}

#[test]
fn counter_parameter_entries_complete_boundary() {
    let text = source(2, true, 0)
        .replace(
            "flag: Boolean",
            "flag: Boolean, own a0: String, own b0: String",
        )
        .replace("\"entry0\"", "a0")
        .replace("\"entry1\"", "b0")
        .replace("run(false)", "run(false, \"a\", \"b\")")
        .replace(
            "while(flag)",
            "var remaining = 2; var again = remaining > 0; while(again)",
        )
        .replace(
            " };",
            " remaining = remaining - 1; again = remaining > 0; };",
        );
    let source = UnitSource::analyze("counter-parameter-entries.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert!(
        actions
            .iter()
            .any(|(kind, _)| *kind == "counter-tail-owner-audit")
    );
    assert!(
        actions
            .iter()
            .any(|(kind, _)| *kind == "counter-tail-complete-boundary-audit")
    );
    super::super::assert_native_output(&source, b"");
}

#[test]
fn counter_exit_permits_do_not_claim_reinitialized_root_drops() {
    let text = source(2, true, 0)
        .replace("while(flag)", "var remaining = 2; var again = remaining > 0; while(again)")
        .replace(" };", " remaining = remaining - 1; again = remaining > 0; }; r0 = \"later0\"; r1 = \"later1\";");
    let source = UnitSource::analyze("counter-exit-reinitialize.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "counter-exit-drop-permit-consumed")
            .count(),
        2
    );
    super::super::assert_native_output(&source, b"");
}

#[test]
fn counter_aliased_parameter_entries_consume_original_catalog() {
    let text = source(2, true, 2)
        .replace(
            "flag: Boolean",
            "flag: Boolean, own a0: String, own b0: String",
        )
        .replace("var r0 = \"entry0\";", "val a1 = a0; var r0 = a1;")
        .replace("var r1 = \"entry1\";", "val b1 = b0; var r1 = b1;")
        .replace("run(false)", "run(false, \"a\", \"b\")")
        .replace(
            "while(flag)",
            "var remaining = 2; var again = remaining > 0; while(again)",
        )
        .replace(
            " };",
            " remaining = remaining - 1; again = remaining > 0; };",
        );
    let source = UnitSource::analyze("counter-alias-parameter-entry.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "counter-entry-permit-consumed")
            .count(),
        2
    );
    super::super::assert_native_output(&source, b"");
}

#[test]
fn two_counter_controls_consume_the_whole_selected_batch() {
    let segment = |i| {
        format!(
            "var n{i} = n; var g{i} = n{i} > 0; var a{i} = \"a\"; var b{i} = \"b\"; while(g{i}) {{ val x{i} = replace(&a{i}, \"x\"); val y{i} = replace(&b{i}, \"y\"); n{i} = n{i} - 1; g{i} = n{i} > 0; }};"
        )
    };
    for suffix in ["", "val copy = 1"] {
        let text = format!(
            "fun run(n: Int) {{ {} {} {suffix} }}\nfun entry() {{ run(0); println(\"ok\"); run(2); println(\"ok\") }}",
            segment(0),
            segment(1)
        );
        let source = UnitSource::analyze("counter-multiple-controls.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert!(
            source
                .ownership()
                .owner_observations()
                .iter()
                .all(|o| o.counter_tail_evidence_v1().is_none())
        );
        let batches: Vec<_> = source
            .ownership()
            .owner_observations()
            .iter()
            .filter_map(|o| o.counter_batch_evidence_v1())
            .collect();
        assert_eq!(batches.len(), usize::from(suffix.is_empty()));
        if let Some(batch) = batches.first() {
            assert_eq!(batch.members().len(), 2);
        }
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        if suffix.is_empty() {
            for kind in [
                "counter-entry-permit-consumed",
                "counter-replace-permit-consumed",
                "counter-binding-permit-consumed",
                "counter-drop-permit-consumed",
                "counter-exit-drop-permit-consumed",
            ] {
                assert_eq!(
                    actions.iter().filter(|(action, _)| *action == kind).count(),
                    4,
                    "{kind}"
                );
            }
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "counter-tail-complete-boundary-audit")
                    .count(),
                2
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "counter-batch-complete-audit")
                    .count(),
                1
            );
        } else {
            assert!(
                actions
                    .iter()
                    .all(|(kind, _)| !kind.starts_with("counter-"))
            );
        }
        super::super::assert_native_output(&source, b"ok\nok\n");
    }
}

fn parameter_batch_source() -> &'static str {
    r#"fun run(n: Int, own seed: String) {
        var n0 = n; var g0 = n0 > 0; var a0 = seed; var b0 = "b";
        while(g0) { val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 };
        var n1 = n; var g1 = n1 > 0; var a1 = "a"; var b1 = "b";
        while(g1) { val x = replace(&a1, "x"); val y = replace(&b1, "y"); n1 = n1 - 1; g1 = n1 > 0 }
    }
    fun entry() { run(-2, "seed"); run(0, "seed"); run(2, "seed"); println("ok") }"#
}

#[test]
fn counter_batch_with_first_parameter_member_consumes_whole_group() {
    for aliased in [false, true] {
        let text = if aliased {
            parameter_batch_source().replace("var a0 = seed;", "val saved = seed; var a0 = saved;")
        } else {
            parameter_batch_source().to_owned()
        };
        let source = UnitSource::analyze("counter-batch-parameter.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            source
                .ownership()
                .owner_observations()
                .iter()
                .filter_map(|o| o.counter_batch_evidence_v1())
                .count(),
            1
        );
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        assert!(
            actions
                .iter()
                .any(|(kind, _)| *kind == "counter-batch-complete-audit")
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "counter-tail-complete-boundary-audit")
                .count(),
            2
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "counter-entry-permit-consumed")
                .count(),
            4
        );
        super::super::assert_native_output(&source, b"ok\n");
    }
}

#[test]
fn ci_only_counter_parameter_batch_rejects_incomplete_entry_and_foreign_root() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source =
        UnitSource::analyze("counter-parameter-batch-fault.ko", parameter_batch_source()).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::MissingCounterEntryPermit,
        OwnerFault::CounterEntryWrongSource,
        OwnerFault::CounterEntryDuplicateCommit,
        OwnerFault::CounterBoundaryWrongEntry,
        OwnerFault::CounterParameterLateRootUse,
        OwnerFault::CounterBatchSecondLateAbort,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| !kind.starts_with("counter-tail-")
                    && *kind != "counter-batch-complete-audit")
        );
    }
}

#[test]
fn ci_only_counter_batch_rejects_foreign_permit_and_late_second_failure_atomically() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let segment = |i| {
        format!(
            r#"var n{i} = n; var g{i} = n{i} > 0; var a{i} = "a"; var b{i} = "b";
        while(g{i}) {{ val x = replace(&a{i}, "x"); val y = replace(&b{i}, "y"); n{i} = n{i} - 1; g{i} = n{i} > 0 }};"#
        )
    };
    let text = format!(
        "fun run(n: Int) {{ {} {} }}\nfun entry() {{ run(2) }}",
        segment(0),
        segment(1)
    );
    let source = UnitSource::analyze("counter-batch-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::CounterPermitWrongIndex,
        OwnerFault::CounterPermitWrongControl,
        OwnerFault::CounterPermitOtherMember,
        OwnerFault::CounterBatchSecondLateAbort,
    ] {
        let (lowered, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(lowered.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| !kind.starts_with("counter-tail-")
                    && *kind != "counter-batch-complete-audit")
        );
    }
}

fn longer_counter_source(count: usize) -> String {
    let segments = (0..count).map(|i| {
            let initial = if i == 0 { "seed" } else { "\"a\"" };
            format!(r#"var n{i} = n; var g{i} = n{i} > 0; var a{i} = {initial}; var b{i} = "b";
                while(g{i}) {{ val x = replace(&a{i}, "x"); val y = replace(&b{i}, "y"); n{i} = n{i} - 1; g{i} = n{i} > 0 }};"#)
        }).collect::<Vec<_>>().join(" ");
    format!(
        "fun run(n: Int, own seed: String) {{ {segments} }}\nfun entry() {{ run(0, \"seed\"); run(2, \"seed\"); println(\"ok\") }}"
    )
}
#[test]
fn longer_counter_batches_consume_all_control_qualified_plans() {
    for count in [3, 4] {
        let text = longer_counter_source(count);
        let source = UnitSource::analyze("long-counter-plans.ko", &text).unwrap();
        source.assert_frontend_clean();
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "counter-tail-complete-boundary-audit")
                .count(),
            count
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "counter-entry-permit-consumed")
                .count(),
            count * 2
        );
        assert!(actions.contains(&("counter-batch-complete-audit", count)));
        super::super::assert_native_output(&source, b"ok\n");
    }
}

#[test]
fn ci_only_longer_counter_batch_rejects_foreign_member_and_middle_failure() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let text = longer_counter_source(4);
    let source = UnitSource::analyze("long-counter-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::CounterPermitWrongIndex,
        OwnerFault::CounterPermitWrongControl,
        OwnerFault::CounterPermitOtherMember,
        OwnerFault::CounterBatchSecondLateAbort,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| !kind.starts_with("counter-tail-")
                    && *kind != "counter-batch-complete-audit")
        );
    }
}
