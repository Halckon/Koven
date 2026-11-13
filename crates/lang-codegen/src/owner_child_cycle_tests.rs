//! Ordinary static receipt evidence; finite execution is not an induction proof.
use super::UnitSource;
#[test]
fn child_cycle_receipt_connects_actual_current_and_next_root_vectors() {
    for replacements in [
        r#"val x=replace(&a,"next-a"); val y=replace(&b,"next-b");"#,
        r#"val y=replace(&b,"next-b"); val x=replace(&a,"next-a");"#,
    ] {
        let text = format!(
            r#"fun run(outer: Int, inner: Int, own seed: String) {{
            var a=seed; var b="b"; var n0=outer; var g0=n0>0;
            while(g0) {{ var n1=inner; var g1=n1>0;
                while(g1) {{ {replacements} n1=n1-1; g1=n1>0 }};
                val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
            }}; println(a); println(b)
        }}
        fun entry() {{ run(0,0,"seed"); run(1,2,"seed") }}"#
        );
        let source = UnitSource::analyze("child-cycle.ko", &text).unwrap();
        source.assert_frontend_clean();
        let product = source
            .ownership()
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap();
        assert!(product.child_generation_rule_v1().is_some());
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = lowered.unwrap_or_else(|error| panic!("{error:?}; actions={actions:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        for marker in [
            "nested-body-retirement-ssa-complete",
            "child-generation-source-ssa-complete",
            "child-cycle-ssa-complete",
            "child-parent-entry-ssa-complete",
            "parent-initial-owner-base-ssa-complete",
            "parent-generation-composition-ssa-complete",
        ] {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(name, _)| *name == marker)
                    .map(|(_, count)| *count)
                    .collect::<Vec<_>>(),
                [2]
            );
        }
    }
}

fn fault_source() -> UnitSource {
    source_with_initial("var a=seed;")
}
fn source_with_initial(initial: &str) -> UnitSource {
    source_with_initial_and_suffix(initial, "println(a); println(b)")
}
fn source_with_initial_and_suffix(initial: &str, suffix: &str) -> UnitSource {
    UnitSource::analyze(
        "child-cycle-fault.ko",
        &r#"fun run(outer:Int,inner:Int,own seed:String) {
        var a=seed; var b="b"; var n0=outer; var g0=n0>0;
        while(g0) { var n1=inner; var g1=n1>0;
            while(g1) { val x=replace(&a,"next-a"); val y=replace(&b,"next-b"); n1=n1-1; g1=n1>0 };
            val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
        }; println(a); println(b)
    }
    fun entry() { run(1,2,"seed") }"#
            .replace("var a=seed;", initial)
            .replace("println(a); println(b)", suffix),
    )
    .unwrap()
}
fn assert_cycle_faults(faults: &[crate::ssa::OwnerFault]) {
    assert_cycle_faults_with_source(fault_source(), faults);
}
fn assert_cycle_faults_with_source(source: UnitSource, faults: &[crate::ssa::OwnerFault]) {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    source.assert_frontend_clean();
    source
        .lower_entry(source.entry())
        .expect("uncorrupted baseline");
    for &fault in faults {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err(), "real corruption must reject: {actions:?}");
        for marker in ["child-cycle-fault-hit", "child-cycle-independent-rejection"] {
            assert_eq!(
                actions.iter().filter(|(name, _)| *name == marker).count(),
                1,
                "{marker}: {actions:?}"
            );
        }
    }
}
#[test]
#[ignore = "CI-only fault injection; explicit child cycle dual-host CI step"]
fn ci_only_child_cycle_rejects_missing_evidence_and_transport_corruption() {
    use crate::ssa::OwnerFault::*;
    assert_cycle_faults(&[
        ChildGenerationProducer,
        ChildGenerationSharedLoan,
        ChildGenerationExtraLoanUse,
        ChildGenerationDuplicateBorrowPlacement,
        ParentCompositionSelectedFlagLost,
        ParentCompositionRequiredFlagLost,
        ParentCompositionMissingCarry,
        ParentCompositionMissingChecked,
        ParentCompositionLostProducer,
        ParentCompositionCarrySwap,
        ParentCompositionUnchangedConsume,
        ParentCompositionSharedLoan,
        ParentCompositionNewConsume,
        ParentCompositionDuplicateProducer,
        ChildCycleInitialMissingPlan,
        ChildCycleInitialSwap,
        ChildCycleInitialConsumer,
        ChildCycleParentTrueSwap,
        ChildCycleParentEntryPredecessor,
        ChildCycleParentEntryConsumer,
        ChildCycleMissingPlan,
        ChildCycleMissingReceipt,
        ChildCycleMissingObservation,
        ChildCycleEntrySwap,
        ChildCycleTrueSwap,
        ChildCycleBackedgeSwap,
        ChildCycleHeaderGuard,
        ChildCycleStaleSuccess,
        ChildCycleMissingSuccessRoot,
        ChildCycleFalseSwap,
    ]);
}
#[test]
#[ignore = "CI-only fault injection; explicit child cycle dual-host CI step"]
fn ci_only_child_cycle_rejects_abort_topology_and_new_owner_corruption() {
    use crate::ssa::OwnerFault::*;
    assert_cycle_faults(&[
        ChildCycleCheckedPolarity,
        ChildCycleAbortBackedge,
        ChildCycleExtraPredecessor,
        ChildCycleNewConsumer,
        ChildCycleDuplicateAbortRoot,
    ]);
}

#[test]
fn initial_base_does_not_claim_used_roots_or_other_producers() {
    for initial in ["var a=seed; println(a);", "var a=seed+\"extra\";"] {
        let source = source_with_initial(initial);
        source.assert_frontend_clean();
        let (result, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = result.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        assert!(
            !actions
                .iter()
                .any(|(name, _)| *name == "parent-initial-owner-base-ssa-complete")
        );
    }
}

#[test]
fn parent_composition_reuses_current_generation_false_partitions() {
    for initial in ["var a=seed;", "var a=\"left\"+\"right\";"] {
        for suffix in ["println(a)", ""] {
            let source = source_with_initial_and_suffix(initial, suffix);
            source.assert_frontend_clean();
            let (result, actions) =
                crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
            let (program, _) = result.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
            crate::ssa::verify::verify_program(&program).unwrap();
            assert_eq!(
                actions
                    .iter()
                    .filter(|(name, _)| *name == "parent-initial-concat-source-ssa-complete")
                    .count(),
                usize::from(initial.contains('+')),
                "{actions:?}"
            );
            assert!(
                actions.iter().any(|(name, count)| *name
                    == "parent-generation-composition-ssa-complete"
                    && *count == 2),
                "{actions:?}"
            );
            assert!(
                actions
                    .iter()
                    .any(|(name, count)| *name == "nested-parent-false-ssa-complete"
                        && *count == if suffix.is_empty() { 2 } else { 1 }),
                "{actions:?}"
            );
        }
    }
}

#[test]
fn literal_concat_entry_has_closed_frontend_generation_evidence() {
    let source = source_with_initial("var a=\"left\"+\"right\";");
    source.assert_frontend_clean();
    let nested = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .expect("nested evidence");
    assert!(
        nested.child_generation_rule_v1().is_some(),
        "closed concat owner entry"
    );
    let (result, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = result.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
    crate::ssa::verify::verify_program(&program).unwrap();
    for marker in [
        "parent-initial-concat-source-ssa-complete",
        "parent-generation-composition-ssa-complete",
    ] {
        assert!(
            actions.iter().any(|(name, _)| *name == marker),
            "{marker}: {actions:?}"
        );
    }
}

#[test]
#[ignore = "CI-only fault injection; explicit child cycle dual-host CI step"]
fn ci_only_initial_concat_rejects_source_cleanup_and_transport_corruption() {
    use crate::ssa::OwnerFault::*;
    assert_cycle_faults_with_source(
        source_with_initial("var a=\"left\"+\"right\";"),
        &[
            InitialConcatDuplicateInput,
            InitialConcatMissingDrop,
            InitialConcatEarlyDrop,
            ChildCycleInitialSwap,
            ChildCycleInitialConsumer,
        ],
    );
}

#[test]
fn initial_concat_native_preserves_zero_and_repeated_generations() {
    // Execution is a regression check only, not the symbolic induction proof.
    // Both an unchanged parent root and the parent-resumed root receive concat.
    for resumed_root in [false, true] {
        let mut calls = String::new();
        let mut expected = String::new();
        for outer in [0, 1, 2] {
            for inner in [0, 1, 2] {
                calls.push_str(&format!("run({outer},{inner});"));
                let a = if outer > 0 && inner > 0 {
                    "next-a"
                } else if resumed_root {
                    "a"
                } else {
                    "leftright"
                };
                let b = if outer > 0 {
                    "outer-b"
                } else if resumed_root {
                    "leftright"
                } else {
                    "b"
                };
                expected.push_str(&format!("{a}\n{b}\n"));
            }
        }
        let initial = if resumed_root {
            r#"var a="a"; var b="left"+"right";"#
        } else {
            r#"var a="left"+"right"; var b="b";"#
        };
        let text = format!(
            r#"fun run(outer:Int,inner:Int) {{
            {initial} var n0=outer; var g0=n0>0;
            while(g0) {{ var n1=inner; var g1=n1>0;
                while(g1) {{ val x=replace(&a,"next-a"); val y=replace(&b,"next-b"); n1=n1-1; g1=n1>0 }};
                val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
            }}; println(a); println(b)
        }}
        fun entry() {{ {calls} }}"#
        );
        let source = UnitSource::analyze("concat-root-position.ko", &text).unwrap();
        source.assert_frontend_clean();
        let (result, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = result.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        for (marker, count) in [
            ("parent-initial-concat-source-ssa-complete", 1),
            ("parent-generation-composition-ssa-complete", 2),
        ] {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(name, _)| *name == marker)
                    .map(|(_, count)| *count)
                    .collect::<Vec<_>>(),
                [count]
            );
        }
        super::assert_general_counter_baseline(&text, expected.as_bytes(), false);
    }
}
