//! Actual straight-line cleanup of second-control live roots, not runtime token induction.
use super::UnitSource;
fn source(suffix: &str) -> UnitSource {
    source_initial(suffix, "n")
}
fn source_initial(suffix: &str, initial: &str) -> UnitSource {
    let text = format!(
        r#"fun run(n: Int) {{
        var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
        while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
        var n1 = {initial}; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
        while(g1) {{ val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }};
        {suffix}
        println("done")
    }}
    fun entry() {{ run(0); run(2) }}"#
    );
    let source = UnitSource::analyze("bridge-suffix.ko", &text).unwrap();
    source.assert_frontend_clean();
    source
}
#[test]
fn bridge_suffix_consumes_exact_live_root_bindings_and_drops() {
    for (suffix, count) in [
        ("val final_a = a1;", 1),
        ("val final_a = a1; val final_b = b1;", 2),
        ("val final_b = b1; val final_a = a1;", 2),
        (
            "val final_b = b1; val final_a = a1; a1 = \"new\"; b1 = \"new\";",
            2,
        ),
    ] {
        let source = source(suffix);
        let bridge = source
            .ownership()
            .owner_observations()
            .iter()
            .find_map(|o| o.control_bridge_evidence_v1())
            .unwrap();
        assert_eq!(
            bridge
                .next_false_exit_suffix()
                .expect("actual complete suffix")
                .len(),
            count
        );
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        for kind in [
            "bridge-suffix-binding-consumed",
            "bridge-suffix-drop-consumed",
        ] {
            assert_eq!(actions.iter().filter(|(k, _)| *k == kind).count(), count);
        }
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "bridge-suffix-complete")
                .count(),
            1
        );
        super::super::super::assert_native_output(&source, b"done\ndone\n");
    }
}
#[test]
fn ci_only_bridge_suffix_rejects_missing_wrong_and_duplicate_permits() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = source("val final_b = b1; val final_a = a1;");
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
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "bridge-suffix-complete"
                    && *kind != "control-bridge-slot-audit")
        );
    }
}

#[test]
fn bridge_counter_entry_consumes_actual_parameter_or_literal_before_guard() {
    for initial in ["n", "0", "2"] {
        let source = source_initial("println(a1); println(b1);", initial);
        let bridge = source
            .ownership()
            .owner_observations()
            .iter()
            .find_map(|o| o.control_bridge_evidence_v1())
            .unwrap();
        assert!(bridge.next_counter_entry().is_some());
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "bridge-entry-binding-consumed")
                .count(),
            2
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "bridge-counter-entry-complete")
                .count(),
            1
        );
        let expected = match initial {
            "n" => "a\nb\ndone\nx1\ny1\ndone\n",
            "0" => "a\nb\ndone\nx\ny\ndone\n",
            _ => "x1\ny1\ndone\nx1\ny1\ndone\n",
        };
        super::super::super::assert_native_output(&source, expected.as_bytes());
    }
}
#[test]
fn ci_only_bridge_counter_entry_requires_actual_bindings_and_comparison() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = source_initial("val a = a1; val b = b1;", "2");
    for fault in [
        OwnerFault::MissingBridgeEntryBinding,
        OwnerFault::BridgeEntryWrongSource,
        OwnerFault::BridgeEntryLateWrongComparison,
        OwnerFault::BridgeEntryLateWrongLiteral,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "bridge-counter-entry-complete"
                    && *kind != "control-bridge-slot-audit")
        );
    }
}

#[test]
fn ci_only_bridge_ssa_transport_rejects_wrong_root_missing_edge_and_abort_backedge() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = source("val final_a = a1; val final_b = b1;");
    for fault in [
        OwnerFault::BridgeSsaWrongRootArgument,
        OwnerFault::BridgeSsaMissingTerminator,
        OwnerFault::BridgeSsaAbortBackedge,
        OwnerFault::BridgeSsaExtraDrop,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "bridge-ssa-fault-injected")
                .count(),
            1
        );
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "bridge-ssa-receipt-complete")
        );
    }
}
