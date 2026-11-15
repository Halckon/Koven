//! Real branch inputs; no mutation, synthetic facts, execution or alternate analysis mixing.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn original_both_arm_paths_close_with_distinct_callees_and_run_isolation() {
    for reverse in [false, true] {
        let audits = run(false, reverse);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert_eq!(audit.expected.len(), 1);
            assert_eq!(audit.linked, audit.expected);
            assert!(audit.gaps.is_empty(), "{:?}", audit.gaps);
        }
    }
}
#[test]
fn original_tail_continuation_still_lowers_but_is_outside_new_domain() {
    for audit in run(true, false) {
        assert_eq!(audit.expected.len(), 1);
        assert!(audit.linked.is_empty());
        assert_eq!(audit.gaps.len(), 1);
        assert!(audit.gaps[0].contains("outside BothArmsReturn"));
    }
}
fn run(tail: bool, reverse: bool) -> Vec<BranchConsumption> {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "provider.ko",
        "fun view_left(source: String): borrow String from source = source\nfun view_right(source: String): borrow String from source = source",
    );
    let text = "fun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view_left(source) } else { return view_right(source) } }\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, flag); consume(item) }";
    let text = if tail {
        text.replace(
            "} else { return view_right(source) }",
            "}; return view_right(source)",
        )
    } else {
        text.to_owned()
    };
    let (caller_id, caller) = unit_lower_test_support::parsed(&mut sources, "caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "caller.ko", caller_id, &caller),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap()
        .id();
    let expected_item = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "wrap")
        .unwrap();
    let item = UnitItemId::new(expected_item.source_unit(), expected_item.root());
    let regions: Vec<_> = owned
        .ownership()
        .loan_action_observations()
        .iter()
        .flat_map(|c| c.branch_forward_regions())
        .collect();
    assert_eq!(regions.len(), 1);
    assert_eq!(
        owned.ownership().d10_branch_link(regions[0].split()),
        Ok(item)
    );
    let ((results, receipts), audits) = crate::ssa::d10_project_audit::observe(|| {
        crate::ssa::refine::branch_receipt::observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, &owned, entry,
                    )
                })
                .collect::<Vec<_>>()
        })
    });
    // Existing publication remains one receipt per original successful assembly.
    assert_eq!(receipts.len(), 2);
    for receipt in receipts {
        assert_ne!(receipt.paths[0].callee, receipt.paths[1].callee);
    }
    for result in results {
        result.unwrap();
    }
    audits.into_iter().map(|a| a.branch_consumption).collect()
}
