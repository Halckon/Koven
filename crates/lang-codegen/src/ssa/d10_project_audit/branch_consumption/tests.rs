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
fn original_tail_continuation_closes_actual_fold_with_distinct_callees() {
    for reverse in [false, true] {
        let audits = run(true, reverse);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert_eq!(audit.expected.len(), 1);
            assert_eq!(audit.linked, audit.expected);
            assert!(audit.gaps.is_empty(), "{:?}", audit.gaps);
        }
    }
}
#[test]
fn certified_branch_and_identity_edges_close_plan_but_keep_entry_lifetime_gap() {
    for tail in [false, true] {
        let audits = run_entry(tail, true, true, false);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert!(audit.gaps.is_empty());
        }
    }
}
#[test]
fn extra_entry_parameter_keeps_actual_call_outside_certificate_domain() {
    for reverse in [false, true] {
        let audits = run_entry(false, reverse, false, true);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert!(audit.gaps.is_empty(), "{:?}", audit.gaps);
        }
    }
}
fn run(tail: bool, reverse: bool) -> Vec<BranchConsumption> {
    run_entry(tail, reverse, false, false)
}
fn run_entry(
    tail: bool,
    reverse: bool,
    wrapper_entry: bool,
    extra_parameter: bool,
) -> Vec<BranchConsumption> {
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
    let text = if extra_parameter {
        text.replace(
            "fun run(flag: Boolean)",
            "fun run(flag: Boolean, unused: Boolean)",
        )
    } else {
        text
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
        .find(|d| d.name() == if wrapper_entry { "wrap" } else { "run" })
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
    for (receipt, result) in receipts.into_iter().zip(results) {
        assert_ne!(receipt.paths[0].callee, receipt.paths[1].callee);
        assert_eq!(receipt.folded_distinct_blocks(), tail);
        assert!(receipt.paths[0].branch.then_arm());
        assert!(!receipt.paths[1].branch.then_arm());
        let (program, entry_function) = result.unwrap();
        for path in &receipt.paths {
            assert_ne!(path.callee, entry_function);
            assert_ne!(
                path.callee.index, 0,
                "identity evidence must support nonzero leaf IDs"
            );
        }
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == receipt.caller)
            .unwrap();
        assert_eq!(function.blocks.len(), 3);
        let branch = function
            .block(receipt.entry)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap();
        let crate::ssa::model::TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &branch.kind
        else {
            panic!("original dynamic branch");
        };
        assert_eq!(when_true, &receipt.paths[0].edge);
        assert_eq!(when_false, &receipt.paths[1].edge);
        for path in &receipt.paths {
            assert_eq!(
                function
                    .block(path.block)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind,
                crate::ssa::model::TerminatorKind::BorrowReturn { loan: path.result }
            );
        }
    }
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    for audit in &audits {
        let mut edges = audit.borrow_edges.clone();
        edges.sort();
        let mut expected_edges = vec![
            (declaration("wrap"), declaration("view_left")),
            (declaration("wrap"), declaration("view_right")),
        ];
        if !wrapper_entry && !extra_parameter {
            expected_edges.push((declaration("run"), declaration("wrap")));
        }
        expected_edges.sort();
        assert_eq!(edges, expected_edges);
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("unclassified terminator: Some(BorrowReturn"))
                .count(),
            0,
            "identity leaf returns require their independent certificate: {:?}",
            audit.unsupported
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("unclassified operation: BorrowCall"))
                .count(),
            usize::from(extra_parameter),
            "run -> wrap requires its independent actual entry certificate"
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .any(|g| g.contains("entry/certified-edge closure")),
            extra_parameter
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("entry binding lifetime unproven"))
                .count(),
            usize::from(!wrapper_entry && !extra_parameter)
        );
        assert!(
            !audit.unsupported.iter().any(|g| g.contains("identity")),
            "{:?}",
            audit.unsupported
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("missing/duplicate entry call receipt"))
                .count(),
            usize::from(extra_parameter),
            "{:?}",
            audit.unsupported
        );
        assert!(
            !audit
                .unsupported
                .iter()
                .any(|g| g.contains("unconsumed certified"))
        );
        if !extra_parameter {
            assert_eq!(
                audit.unsupported.len(),
                usize::from(!wrapper_entry),
                "{:?}",
                audit.unsupported
            );
        }
        if wrapper_entry {
            assert!(audit.direct_edges.is_empty());
        }
    }
    audits.into_iter().map(|a| a.branch_consumption).collect()
}
