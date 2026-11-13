//! Counter boundary consumption beyond the scalar/body audit.
use super::{UnitSource, source};
#[path = "owner_control_chain_tests.rs"]
mod chain;
#[path = "owner_child_cycle_tests.rs"]
mod child_cycle;
#[path = "owner_control_bridge_exit_tests.rs"]
mod exit_partition;
#[path = "owner_nested_body_tests.rs"]
mod nested_body;

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

// These are legacy lowering baselines, not new cross-control owner permissions.
#[test]
fn live_roots_cross_sequential_counters_without_fabricated_batch() {
    for aliased in [false, true] {
        let bridge = if aliased {
            "val saved_b = b0; val saved_a = a0; var b1 = saved_b; var a1 = saved_a;"
        } else {
            "var a1 = a0; var b1 = b0;"
        };
        let mut calls = String::new();
        let mut expected = String::new();
        for first in [-2, 0, 2] {
            for second in [-2, 0, 1, 2] {
                calls.push_str(&format!("run({first}, {second}, \"seed\");"));
                let (a, b) = if second > 0 {
                    ("second-a", "second-b")
                } else if first > 0 {
                    ("first-a", "first-b")
                } else {
                    ("seed", "entry-b")
                };
                expected.push_str(&format!("{a}\n{b}\n"));
            }
        }
        let text = format!(
            r#"fun run(first: Int, second: Int, own seed: String) {{
            var n0 = first; var g0 = n0 > 0; var a0 = seed; var b0 = "entry-b";
            while(g0) {{ val x = replace(&a0, "first-a"); val y = replace(&b0, "first-b");
                n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = second; var g1 = n1 > 0; {bridge}
            while(g1) {{ val x = replace(&a1, "second-a"); val y = replace(&b1, "second-b");
                n1 = n1 - 1; g1 = n1 > 0 }};
            println(a1); println(b1)
        }}
        fun entry() {{ {calls} }}"#
        );
        let text = if aliased {
            text.replace(
                "val x = replace(&a1, \"second-a\"); val y = replace(&b1, \"second-b\");",
                "val y = replace(&b1, \"second-b\"); val x = replace(&a1, \"second-a\");",
            )
        } else {
            text
        };
        assert_general_counter_baseline(&text, expected.as_bytes(), true);
    }
}

fn assert_general_counter_baseline(text: &str, expected: &[u8], bridge: bool) {
    let source = UnitSource::analyze("cross-control-counter.ko", text).unwrap();
    source.assert_frontend_clean();
    for observation in source.ownership().owner_observations() {
        assert!(observation.counter_batch_evidence_v1().is_none());
        assert!(observation.counter_tail_evidence_v1().is_none());
        assert!(observation.multi_root_boundary_v1().is_none());
    }
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert!(
        actions
            .iter()
            .all(|(kind, _)| !kind.starts_with("counter-"))
    );
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "control-bridge-slot-audit")
            .count(),
        usize::from(bridge)
    );
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "bridge-ssa-receipt-complete")
            .count(),
        usize::from(bridge)
    );
    let nested = source
        .ownership()
        .owner_observations()
        .iter()
        .any(|o| o.nested_normal_body_v1().is_some());
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "nested-owner-normal-body-complete")
            .count(),
        usize::from(nested)
    );
    let chain = source
        .ownership()
        .owner_observations()
        .iter()
        .any(|o| o.control_chain_bodies_v1().is_some());
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "control-chain-bodies-complete")
            .count(),
        usize::from(chain)
    );
    if chain {
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-exits-complete")
                .count(),
            1
        );
        let expected_steps: usize = source
            .ownership()
            .owner_observations()
            .iter()
            .filter_map(|o| o.control_chain_bodies_v1())
            .map(|g| {
                g.leading().normal().steps().len()
                    + g.members()
                        .iter()
                        .map(|m| m.normal().steps().len())
                        .sum::<usize>()
            })
            .sum();
        for kind in [
            "bridge-body-replace-consumed",
            "bridge-body-binding-consumed",
            "bridge-body-drop-consumed",
        ] {
            assert_eq!(
                actions.iter().filter(|(actual, _)| *actual == kind).count(),
                expected_steps
            );
        }
    }
    if bridge {
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-counter-tail-complete")
                .count(),
            1
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-body-complete")
                .count(),
            1
        );
        for kind in [
            "bridge-body-replace-consumed",
            "bridge-body-binding-consumed",
            "bridge-body-drop-consumed",
        ] {
            assert_eq!(
                actions.iter().filter(|(actual, _)| *actual == kind).count(),
                2
            );
        }
        assert_cross_control_ssa(&source, &program, &[("a", "a"), ("b", "b")]);
    }
    super::super::assert_native_output(&source, expected);
}

// Check final typed-SSA edge operands, not only the pre-SSA slot audit.
fn assert_cross_control_ssa(
    source: &UnitSource,
    program: &crate::ssa::model::Program,
    roots: &[(&str, &str)],
) {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    let run = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .unwrap()
                .starts_with("fun run(")
        })
        .unwrap();
    for (previous, next_name) in roots {
        let replace = |name, suffix| {
            run.instructions
                .iter()
                .find(|i| {
                    matches!(i.operation, Operation::RootReplace { .. })
                        && source
                            .sources
                            .slice(i.origin.span())
                            .unwrap()
                            .starts_with(&format!("replace(&{name}{suffix},"))
                })
                .unwrap()
        };
        let prior = replace(previous, 0);
        let next = replace(next_name, 1);
        let Operation::RootReplace {
            owner: original, ..
        } = prior.operation
        else {
            unreachable!()
        };
        let Operation::RootReplace { owner: target, .. } = next.operation else {
            unreachable!()
        };
        let body = run.block(prior.block).unwrap();
        let position = body
            .parameters
            .iter()
            .position(|v| *v == EntityId::Value(original))
            .unwrap();
        let (header, when_true, when_false) = run
            .blocks
            .iter()
            .find_map(|b| {
                if let TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } = &b.terminator.as_ref()?.kind
                {
                    (when_true.target == body.id).then_some((b, when_true, when_false))
                } else {
                    None
                }
            })
            .unwrap();
        let header_value = when_true.arguments[position];
        assert!(header.parameters.contains(&header_value));
        // The zero-round and checked normal-backedge predecessors both exist.
        let inputs: Vec<_> = run
            .blocks
            .iter()
            .filter_map(|b| {
                if let TerminatorKind::Branch(edge) = &b.terminator.as_ref()?.kind {
                    (edge.target == header.id).then_some(edge)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(inputs.len(), 2);
        let mut edge = when_false;
        let mut carried = header_value;
        let mut reached = false;
        for _ in 0..8 {
            let destination = run.block(edge.target).unwrap();
            let index = edge.arguments.iter().position(|v| *v == carried).unwrap();
            carried = destination.parameters[index];
            if destination.id == next.block {
                assert_eq!(carried, EntityId::Value(target));
                reached = true;
                break;
            }
            edge = match &destination.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => edge,
                TerminatorKind::Conditional { when_true, .. } if when_true.target == next.block => {
                    when_true
                }
                other => panic!("unexpected bridge path: {other:?}"),
            };
        }
        assert!(reached);
    }
}

#[test]
fn ci_only_control_bridge_rejects_wrong_target_and_missing_next() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let text = r#"fun run(n: Int) {
        var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
        while(g0) { val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 };
        var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
        while(g1) { val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }
    }
    fun entry() { run(2) }"#;
    let source = UnitSource::analyze("control-bridge-fault.ko", text).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::ControlBridgeWrongRoot,
        OwnerFault::ControlBridgeMissingNext,
        OwnerFault::ControlBridgeLateSourceDrop,
        OwnerFault::ControlBridgeHeaderRootDrop,
        OwnerFault::ControlBridgeWrongBodyEdge,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "control-bridge-slot-audit")
        );
    }
}

#[test]
fn partial_live_exit_bridges_sparse_roots_without_resurrecting_retired_root() {
    for carry in [1, 2] {
        let second_binding = if carry == 2 {
            "var b1 = saved_c;"
        } else {
            "val last_c = saved_c;"
        };
        let second_replace = if carry == 2 {
            "val y = replace(&b1, \"second-c\");"
        } else {
            ""
        };
        let second_print = if carry == 2 { "println(b1);" } else { "" };
        let mut calls = String::new();
        let mut expected = String::new();
        for first in [0, 2] {
            for second in [0, 2] {
                calls.push_str(&format!("run({first}, {second});"));
                let a = if second > 0 {
                    "second-a"
                } else if first > 0 {
                    "first-a"
                } else {
                    "entry-a"
                };
                expected.push_str(&format!("{a}\n"));
                if carry == 2 {
                    let c = if second > 0 {
                        "second-c"
                    } else if first > 0 {
                        "first-c"
                    } else {
                        "entry-c"
                    };
                    expected.push_str(&format!("{c}\n"));
                }
            }
        }
        let text = format!(
            r#"fun run(first: Int, second: Int) {{
            var n0 = first; var g0 = n0 > 0;
            var a0 = "entry-a"; var b0 = "retired-b"; var c0 = "entry-c";
            while(g0) {{ val x = replace(&a0, "first-a"); val y = replace(&b0, "first-b");
                val z = replace(&c0, "first-c"); n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = second; var g1 = n1 > 0;
            val saved_c = c0; var a1 = a0; {second_binding}
            while(g1) {{ val x = replace(&a1, "second-a"); {second_replace} n1 = n1 - 1; g1 = n1 > 0 }};
            println(a1); {second_print}
        }}
        fun entry() {{ {calls} }}"#
        );
        let source = UnitSource::analyze("sparse-control-bridge.ko", &text).unwrap();
        source.assert_frontend_clean();
        let bridges: Vec<_> = source
            .ownership()
            .owner_observations()
            .iter()
            .filter_map(|o| o.control_bridge_evidence_v1())
            .collect();
        assert_eq!(bridges.len(), usize::from(carry == 2));
        if let Some(bridge) = bridges.first() {
            assert_eq!(bridge.prior().root_slots().len(), 3);
            assert_eq!(bridge.sources().len(), 2);
            assert!(bridge.next_normal_transport().is_some());
            assert!(bridge.next_counter_tail().is_some());
            let (live, drops) = bridge.prior().false_exit_partition().unwrap();
            assert_eq!(live.len(), 2);
            assert_eq!(drops.len(), 1);
        }
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-bridge-slot-audit")
                .count(),
            usize::from(carry == 2)
        );
        if carry == 2 {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "bridge-body-complete")
                    .count(),
                1
            );
            assert_cross_control_ssa(&source, &program, &[("a", "a"), ("c", "b")]);
        }
        super::super::assert_native_output(&source, expected.as_bytes());
    }
}

#[test]
fn ci_only_control_bridge_body_chain_fails_closed() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let text = r#"fun run(n: Int) {
        var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
        while(g0) { val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 };
        var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
        while(g1) { val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }
    }
    fun entry() { run(2) }"#;
    let source = UnitSource::analyze("bridge-body-chain.ko", text).unwrap();
    source.assert_frontend_clean();
    assert!(source.ownership().owner_observations().iter().any(|o| {
        o.control_bridge_evidence_v1()
            .and_then(|b| b.next_normal_transport())
            .is_some()
    }));
    for fault in [
        OwnerFault::MissingBridgeExitDropPermit,
        OwnerFault::BridgeExitWrongInstruction,
        OwnerFault::BridgeExitDuplicateCommit,
        OwnerFault::BridgeExitWrongFactTarget,
        OwnerFault::BridgeExitWrongExitEdge,
        OwnerFault::BridgeExitLateWrongOwner,
        OwnerFault::MissingBridgeReplacePermit,
        OwnerFault::MissingBridgeBindingPermit,
        OwnerFault::MissingBridgeDropPermit,
        OwnerFault::BridgeReplaceWrongRoot,
        OwnerFault::BridgeSecondReplaceFailure,
        OwnerFault::BridgeWrongInstruction,
        OwnerFault::BridgeDropDuplicateCommit,
        OwnerFault::BridgeCounterAbortToHeader,
        OwnerFault::BridgeCounterWrongBackedge,
        OwnerFault::BridgeCounterStaleComparison,
        OwnerFault::BridgeCounterWrongWriteback,
        OwnerFault::BridgeCounterMissingReceipt,
        OwnerFault::BridgeCounterForeignSuccessPredecessor,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(actions.iter().all(
            |(kind, _)| *kind != "bridge-body-complete" && *kind != "control-bridge-slot-audit"
        ));
    }
}

#[path = "owner_control_bridge_suffix_tests.rs"]
mod suffix_consumption;
