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
                    .filter(|(kind, _)| *kind == "counter-tail-complete-boundary-audit")
                    .count(),
                1
            );
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
        OwnerFault::CounterBoundaryMissingSuffix,
        OwnerFault::CounterBoundaryWrongEntry,
    ] {
        assert!(with_owner_fault(fault, || source.lower_entry(source.entry())).is_err());
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
fn counter_parameter_entries_keep_existing_scalar_audit() {
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
            .all(|(kind, _)| *kind != "counter-tail-complete-boundary-audit")
    );
    super::super::assert_native_output(&source, b"");
}
