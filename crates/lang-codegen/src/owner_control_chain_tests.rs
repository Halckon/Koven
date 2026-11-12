//! Grouped normal-body permissions do not grant whole-loop or SSA authority.
use super::assert_general_counter_baseline;

#[test]
fn three_live_controls_preserve_zero_and_normal_generations() {
    let mut calls = String::new();
    let mut expected = String::new();
    for rounds in [[0, 0, 0], [2, 0, 0], [0, 2, 0], [2, 0, 2], [2, 2, 2]] {
        calls.push_str(&format!(
            "run({}, {}, {});",
            rounds[0], rounds[1], rounds[2]
        ));
        let latest = rounds.iter().rposition(|n| *n > 0);
        let values = match latest {
            Some(0) => ["a0", "b0"],
            Some(1) => ["a1", "b1"],
            Some(2) => ["a2", "b2"],
            None => ["entry-a", "entry-b"],
            _ => unreachable!(),
        };
        expected.push_str(&format!("{}\n{}\n", values[0], values[1]));
    }
    let text = format!(
        r#"
        fun run(first: Int, second: Int, third: Int) {{
            var n0 = first; var g0 = n0 > 0; var a0 = "entry-a"; var b0 = "entry-b";
            while(g0) {{ val x = replace(&a0, "a0"); val y = replace(&b0, "b0");
                n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = second; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) {{ val x = replace(&a1, "a1"); val y = replace(&b1, "b1");
                n1 = n1 - 1; g1 = n1 > 0 }};
            var n2 = third; var g2 = n2 > 0; var b2 = b1; var a2 = a1;
            while(g2) {{ val y = replace(&b2, "b2"); val x = replace(&a2, "a2");
                n2 = n2 - 1; g2 = n2 > 0 }};
            println(a2); println(b2)
        }}
        fun entry() {{ {calls} }}
    "#
    );
    assert_general_counter_baseline(&text, expected.as_bytes(), false);
    // Closed cleanup selects both continuation bodies as one mandatory group;
    // entry/tail/exit and typed-SSA whole-chain authority remain unavailable.
    let closed = text.replace(
        "println(a2); println(b2)",
        "val final_a = a2; val final_b = b2; println(\"done\")",
    );
    assert_general_counter_baseline(&closed, b"done\ndone\ndone\ndone\ndone\n", false);
}

fn closed_chain_source(sparse: bool) -> super::UnitSource {
    chain_source(sparse, false)
}
fn chain_source(sparse: bool, suffix: bool) -> super::UnitSource {
    chain_source_with_counter(sparse, suffix, "n")
}
fn chain_source_with_counter(sparse: bool, suffix: bool, counter: &str) -> super::UnitSource {
    let mut text = String::from("fun run(n: Int) {");
    for i in 0..3 {
        text.push_str(&format!("var n{i}={counter}; var g{i}=n{i}>0;"));
        if i == 0 {
            text.push_str("var a0=\"a\"; var b0=\"b\";");
        } else {
            let p = i - 1;
            text.push_str(&format!("var a{i}=a{p}; var b{i}=b{p};"));
        }
        if sparse && i < 2 {
            text.push_str(if i == 0 {
                "var c0=\"c\";"
            } else {
                "var c1=c0;"
            });
        }
        let extra = if sparse && i < 2 {
            format!("val z=replace(&c{i},\"z\");")
        } else {
            String::new()
        };
        text.push_str(&format!("while(g{i}) {{ val x=replace(&a{i},\"x\"); val y=replace(&b{i},\"y\"); {extra} n{i}=n{i}-1; g{i}=n{i}>0 }};"));
    }
    if suffix {
        text.push_str("val final_a=a2; val final_b=b2;");
    }
    text.push_str("}\nfun entry() {run(0); run(2)}");
    let source = super::UnitSource::analyze("chain-body-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .any(|o| o.control_chain_bodies_v1().is_some())
    );
    source
}
#[test]
fn closed_chain_body_group_consumes_all_members() {
    let source = closed_chain_source(false);
    let (result, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = result.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, n)| *kind == "control-chain-bodies-complete" && *n == 2)
            .count(),
        1
    );
}
#[test]
fn ci_only_chain_body_group_rejects_foreign_control_and_last_missing_drop() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = closed_chain_source(false);
    for fault in [
        OwnerFault::ChainBodyWrongControl,
        OwnerFault::ChainLastBodyMissingDrop,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-fault-hit")
                .count(),
            1
        );
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "control-chain-bodies-complete")
        );
    }
}

#[test]
fn chain_false_exit_consumes_sparse_middle_and_dead_final_roots() {
    for sparse in [false, true] {
        let source = closed_chain_source(sparse);
        let (result, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-exit-drop-consumed")
                .count(),
            if sparse { 3 } else { 2 }
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, n)| *kind == "control-chain-exits-complete" && *n == 2)
                .count(),
            1
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, n)| *kind == "control-chain-tails-complete" && *n == 2)
                .count(),
            1
        );
        super::super::super::assert_native_output(&source, b"");
    }
}
#[test]
fn ci_only_chain_false_exit_rejects_foreign_control_or_missing_permit() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = closed_chain_source(false);
    for fault in [
        OwnerFault::ChainExitWrongControl,
        OwnerFault::MissingBridgeExitDropPermit,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-exit-fault-hit")
                .count(),
            1
        );
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "control-chain-exits-complete"
                    && *kind != "control-chain-bodies-complete")
        );
    }
}

#[test]
fn ci_only_chain_tail_missing_last_receipt_revokes_whole_group() {
    let source = closed_chain_source(false);
    let (result, actions) = crate::ssa::observe_owner_actions(|| {
        crate::ssa::with_owner_fault(crate::ssa::OwnerFault::ChainLastTailMissingReceipt, || {
            source.lower_entry(source.entry())
        })
    });
    assert!(result.is_err());
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "control-chain-tail-fault-hit")
            .count(),
        1
    );
    assert!(actions.iter().all(|(kind, _)| !matches!(
        *kind,
        "control-chain-tails-complete"
            | "control-chain-exits-complete"
            | "control-chain-bodies-complete"
    )));
}

#[test]
fn chain_final_suffix_consumes_original_bindings_and_drops() {
    for sparse in [false, true] {
        let source = chain_source(sparse, true);
        let (result, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        for action in [
            "bridge-suffix-binding-consumed",
            "bridge-suffix-drop-consumed",
        ] {
            assert_eq!(
                actions.iter().filter(|(kind, _)| *kind == action).count(),
                2
            );
        }
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, n)| *kind == "control-chain-bodies-complete" && *n == 2)
                .count(),
            1
        );
        super::super::super::assert_native_output(&source, b"");
    }
}
#[test]
fn ci_only_chain_final_suffix_rejects_missing_or_reused_consumption() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = chain_source(false, true);
    for fault in [
        OwnerFault::MissingBridgeSuffixBinding,
        OwnerFault::MissingBridgeSuffixDrop,
        OwnerFault::BridgeSuffixWrongSource,
        OwnerFault::BridgeSuffixDuplicateDrop,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-suffix-fault-hit")
                .count(),
            1
        );
        assert!(actions.iter().all(|(kind, _)| !matches!(
            *kind,
            "control-chain-tails-complete"
                | "control-chain-exits-complete"
                | "control-chain-bodies-complete"
        )));
    }
}

#[test]
fn chain_scalar_entries_consume_original_parameter_and_literal_bindings() {
    for counter in ["n", "0", "2"] {
        let source = chain_source_with_counter(true, true, counter);
        let (result, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-entry-binding-consumed")
                .count(),
            4
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, n)| *kind == "control-chain-bodies-complete" && *n == 2)
                .count(),
            1
        );
        super::super::super::assert_native_output(&source, b"");
    }
}
#[test]
fn ci_only_chain_scalar_entries_reject_missing_or_wrong_source() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = chain_source(false, true);
    for fault in [
        OwnerFault::MissingBridgeEntryBinding,
        OwnerFault::BridgeEntryWrongSource,
        OwnerFault::ChainLastEntryMissingBinding,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-entry-fault-hit")
                .count(),
            1
        );
        assert!(actions.iter().all(|(kind, _)| !matches!(
            *kind,
            "control-chain-tails-complete"
                | "control-chain-exits-complete"
                | "control-chain-bodies-complete"
        )));
    }
}

#[test]
fn chain_ssa_members_transport_sparse_roots_and_retired_values() {
    for sparse in [false, true] {
        for suffix in [false, true] {
            let source = chain_source(sparse, suffix);
            let (result, actions) =
                crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
            let (program, _) = result.unwrap();
            crate::ssa::verify::verify_program(&program).unwrap();
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, n)| *kind == "control-chain-ssa-complete" && *n == 2)
                    .count(),
                1
            );
            assert!(
                actions
                    .iter()
                    .all(|(kind, _)| *kind != "bridge-ssa-receipt-complete")
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, n)| *kind == "control-chain-link-complete" && *n == 2)
                    .count(),
                1
            );
            super::super::super::assert_native_output(&source, b"");
        }
    }
}
#[test]
fn ci_only_chain_ssa_last_member_failure_revokes_group_receipt() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = chain_source(true, true);
    for fault in [
        OwnerFault::ChainSsaMissingLastTerminator,
        OwnerFault::ChainSsaLastDropNewGeneration,
        OwnerFault::ChainSsaCrossMemberRootArgument,
        OwnerFault::ChainSsaFoldWrongBody,
        OwnerFault::ChainSsaFoldWrongChecked,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-ssa-fault-hit")
                .count(),
            1
        );
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "control-chain-ssa-complete")
        );
    }
}

#[test]
fn ci_only_chain_link_wrong_target_rejects_group_receipt() {
    let source = chain_source(true, true);
    for fault in [
        crate::ssa::OwnerFault::ChainSsaWrongLinkTarget,
        crate::ssa::OwnerFault::ChainSsaLinkWrongEdgeArguments,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "control-chain-link-fault-hit")
                .count(),
            1
        );
        assert!(actions.iter().all(|(kind, _)| !matches!(
            *kind,
            "control-chain-link-complete" | "control-chain-ssa-complete"
        )));
    }
}
