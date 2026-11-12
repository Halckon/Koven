//! The nested permission only consumes one normal body, not a loop invariant.
use super::{UnitSource, assert_general_counter_baseline};

#[test]
fn ci_only_nested_body_missing_permits_and_foreign_parent_fail_closed() {
    use crate::ssa::OwnerFault;
    let source = UnitSource::analyze(
        "nested-permits.ko",
        r#"
        fun run(outer: Int, inner: Int) {
            var a="a"; var b="b"; var n0=outer; var g0=n0>0;
            while(g0) {
                var n1=inner; var g1=n1>0;
                while(g1) { val x=replace(&a,"new-a"); val y=replace(&b,"new-b");
                    n1=n1-1; g1=n1>0 };
                n0=n0-1; g0=n0>0
            }; println(a); println(b)
        }
        fun entry() { run(2,2) }
    "#,
    )
    .unwrap();
    source.assert_frontend_clean();
    assert_eq!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .filter(|o| o.nested_normal_body_v1().is_some())
            .count(),
        1
    );
    for fault in [
        OwnerFault::MissingBridgeReplacePermit,
        OwnerFault::MissingBridgeBindingPermit,
        OwnerFault::MissingBridgeDropPermit,
        OwnerFault::BridgeDropDuplicateCommit,
        OwnerFault::NestedBodyWrongParent,
        OwnerFault::NestedBodyLateMissingDrop,
        OwnerFault::NestedBodyLateDuplicateDrop,
        OwnerFault::NestedBodyForeignLoan,
        OwnerFault::NestedBodyLateRootPlace,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "nested-owner-normal-body-complete")
        );
        if matches!(
            fault,
            OwnerFault::NestedBodyForeignLoan | OwnerFault::NestedBodyLateRootPlace
        ) {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "nested-preparation-fault-hit")
                    .count(),
                1
            );
        }
        if matches!(
            fault,
            OwnerFault::NestedBodyLateMissingDrop | OwnerFault::NestedBodyLateDuplicateDrop
        ) {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "nested-retirement-fault-hit")
                    .count(),
                1
            );
            assert!(
                actions
                    .iter()
                    .all(|(kind, _)| *kind != "nested-owner-retirement-rechecked")
            );
        }
        if fault == OwnerFault::NestedBodyWrongParent {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "nested-parent-fault-hit")
                    .count(),
                1
            );
        }
    }
}

#[test]
fn nested_counters_preserve_outer_live_roots_without_flat_batch() {
    let mut calls = String::new();
    let mut expected = String::new();
    for outer in [0, 1, 2] {
        for inner in [0, 1, 2] {
            calls.push_str(&format!("run({outer}, {inner}, \"seed\");"));
            let a = if outer > 0 && inner > 0 {
                "inner-a"
            } else {
                "seed"
            };
            let b = if outer > 0 { "outer-b" } else { "entry-b" };
            expected.push_str(&format!("{a}\n{b}\n"));
        }
    }
    let text = format!(
        r#"fun run(outer: Int, inner: Int, own seed: String) {{
        var a = seed; var b = "entry-b"; var n0 = outer; var g0 = n0 > 0;
        while(g0) {{
            var n1 = inner; var g1 = n1 > 0;
            while(g1) {{ val x = replace(&a, "inner-a"); val y = replace(&b, "inner-b");
                n1 = n1 - 1; g1 = n1 > 0 }};
            val old = replace(&b, "outer-b"); n0 = n0 - 1; g0 = n0 > 0
        }};
        println(a); println(b)
    }}
    fun entry() {{ {calls} }}"#
    );
    let source = UnitSource::analyze("nested-normal.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert_eq!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .filter(|o| o.nested_normal_body_v1().is_some())
            .count(),
        1
    );
    assert_nested_ssa_retirement(&source);
    assert_general_counter_baseline(&text, expected.as_bytes(), false);
}

fn assert_nested_ssa_retirement(source: &UnitSource) {
    use crate::ssa::model::{EntityId, Operation};
    let product = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .unwrap();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "nested-owner-retirement-rechecked")
            .map(|(_, count)| *count)
            .collect::<Vec<_>>(),
        [2]
    );
    for step in product.normal().steps() {
        let span = source
            .parsed()
            .ast()
            .expressions()
            .get(step.call().expression())
            .unwrap()
            .span();
        let matches = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .flat_map(|f| {
                f.instructions
                    .iter()
                    .filter(move |i| {
                        i.origin.span() == span
                            && matches!(i.operation, Operation::RootReplace { .. })
                    })
                    .map(move |i| (f, i))
            })
            .collect::<Vec<_>>();
        let [(function, replace)] = matches.as_slice() else {
            panic!("unique actual inner Replace")
        };
        let [new, old] = replace.results.as_slice() else {
            panic!("new/retired pair")
        };
        assert_ne!(new, old);
        let drops = function
            .instructions
            .iter()
            .filter(
                |i| matches!(i.operation, Operation::Drop {owner} if EntityId::Value(owner)==*old),
            )
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].block, replace.block);
        let block = function.block(replace.block).unwrap();
        assert!(
            block
                .instructions
                .iter()
                .position(|id| *id == replace.id)
                .unwrap()
                < block
                    .instructions
                    .iter()
                    .position(|id| *id == drops[0].id)
                    .unwrap()
        );
    }
}
