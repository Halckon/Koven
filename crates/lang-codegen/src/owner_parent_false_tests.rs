//! Independent parent false partition and native execution coverage.
use super::{UnitSource, assert_general_counter_baseline};

#[test]
fn parent_false_receipt_partitions_live_and_retired_roots() {
    for (suffix, keep_a, keep_b) in [
        ("println(a); println(b)", true, true),
        ("println(b)", false, true),
        ("println(a)", true, false),
        ("", false, false),
    ] {
        let live = usize::from(keep_a) + usize::from(keep_b);
        let retired = 2 - live;
        let text = format!(
            r#"fun run(outer: Int, inner: Int, own seed: String) {{
            var a=seed; var b="b"; var n0=outer; var g0=n0>0;
            while(g0) {{ var n1=inner; var g1=n1>0;
                while(g1) {{ val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b");
                    n1=n1-1; g1=n1>0 }};
                val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
            }}; {suffix}
        }}
        fun entry() {{ run(0,0,"seed"); run(1,1,"seed"); run(2,2,"seed"); run(2,0,"seed") }}"#
        );
        let source = UnitSource::analyze("parent-false.ko", &text).unwrap();
        source.assert_frontend_clean();
        let product = source
            .ownership()
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap();
        let p = product.parent_boundary_v1().expect("parent evidence");
        assert_eq!(
            (
                p.false_exit_partition().0.len(),
                p.false_exit_partition().1.len()
            ),
            (live, retired)
        );
        if keep_a && !keep_b {
            // This is the reverse partial partition: retire the root actually
            // replaced by parent resume, not merely any one of two String roots.
            let resumed = product.parent_resume_write().expect("parent resume").root();
            let (survivors, drops) = p.false_exit_partition();
            assert!(!survivors.contains(&resumed));
            assert_eq!(drops.len(), 1);
            assert_eq!(
                drops[0].target(),
                lang_frontend::ownership_checking::UnitDropTarget::Named(resumed)
            );
            assert_eq!(
                drops[0].point(),
                lang_frontend::ownership_checking::UnitDropPoint::LoopExit(p.control())
            );
        }
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, _) = lowered.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "nested-parent-false-root-vector-complete")
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "nested-parent-false-ssa-complete")
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            [retired]
        );
        let mut expected = String::new();
        for (a, b) in [
            ("seed", "b"),
            ("inner-a", "outer-b"),
            ("inner-a", "outer-b"),
            ("seed", "outer-b"),
        ] {
            if keep_a {
                expected.push_str(&format!("{a}\n"));
            }
            if keep_b {
                expected.push_str(&format!("{b}\n"));
            }
        }
        assert_general_counter_baseline(&text, expected.as_bytes(), false);
    }
}

fn fault_source() -> UnitSource {
    UnitSource::analyze("parent-fault.ko", r#"
    fun run(outer:Int,inner:Int) {
        var a="a"; var b="b"; var n0=outer; var g0=n0>0;
        while(g0) { var n1=inner; var g1=n1>0;
            while(g1) { val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b"); n1=n1-1; g1=n1>0 };
            val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
        }; println(b)
    }
    fun entry() { run(2,2) }
    "#).unwrap()
}

#[test]
#[ignore = "CI-only fault injection; explicit parent receipt CI step"]
fn ci_only_parent_header_receipt_rejects_actual_edge_corruption() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    use crate::ssa::OwnerFault;
    let source = fault_source();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::NestedHeaderSwapArguments,
        OwnerFault::NestedHeaderMissingArgument,
        OwnerFault::NestedHeaderExtraPredecessor,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert_eq!(
            actions
                .iter()
                .filter(|(k, _)| *k == "nested-resume-ssa-fault-hit")
                .count(),
            1
        );
        assert!(
            actions
                .iter()
                .all(|(k, _)| *k != "nested-parent-header-vector-complete")
        );
    }
}

#[test]
#[ignore = "CI-only fault injection; explicit parent receipt CI step"]
fn ci_only_parent_false_receipt_rejects_actual_partition_and_consumption_corruption() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    use crate::ssa::OwnerFault;
    let source = fault_source();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::ParentFalseMissingPlan,
        OwnerFault::ParentFalseIncompletePartition,
        OwnerFault::ParentFalseSwapArguments,
        OwnerFault::ParentFalseMissingDrop,
        OwnerFault::ParentFalseSecondDrop,
        OwnerFault::ParentFalseEscape,
        OwnerFault::ParentFalseExtraPredecessor,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        let marker = if matches!(
            fault,
            OwnerFault::ParentFalseMissingPlan | OwnerFault::ParentFalseIncompletePartition
        ) {
            "parent-false-plan-fault-hit"
        } else {
            "parent-false-ssa-fault-hit"
        };
        assert_eq!(actions.iter().filter(|(k, _)| *k == marker).count(), 1);
        assert!(
            actions
                .iter()
                .all(|(k, _)| *k != "nested-parent-false-ssa-complete")
        );
    }
}
