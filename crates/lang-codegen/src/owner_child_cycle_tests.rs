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
            "child-cycle-ssa-complete",
            "child-parent-entry-ssa-complete",
            "parent-initial-owner-base-ssa-complete",
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
            .replace("var a=seed;", initial),
    )
    .unwrap()
}
fn assert_cycle_faults(faults: &[crate::ssa::OwnerFault]) {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let source = fault_source();
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
