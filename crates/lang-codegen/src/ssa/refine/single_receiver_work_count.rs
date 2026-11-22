//! Test-only row counts. These are neither timings nor full compiler complexity.
use crate::ssa::unit_lower_test_support;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
use std::{cell::Cell, collections::HashSet, fmt::Write};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ssa::refine) struct Counts {
    pub(in crate::ssa::refine) route_bindings: usize,
    pub(in crate::ssa::refine) route_ends: usize,
    pub(in crate::ssa::refine) route_drops: usize,
    pub(in crate::ssa::refine) precheck_ends: usize,
    pub(in crate::ssa::refine) receiver_rows: usize,
    pub(in crate::ssa::refine) receiver_probes: usize,
    pub(in crate::ssa::refine) subset_drop_index_rows: usize,
    pub(in crate::ssa::refine) subset_drop_rows: usize,
    pub(in crate::ssa::refine) subset_conditional_drop_rows: usize,
    pub(in crate::ssa::refine) declaration_rows: usize,
    pub(in crate::ssa::refine) region_rows: usize,
    pub(in crate::ssa::refine) drop_rows: usize,
    pub(in crate::ssa::refine) region_candidates: usize,
    pub(in crate::ssa::refine) drop_candidates: usize,
}

impl Counts {
    fn without_subset_drops(self) -> Self {
        Self {
            subset_drop_index_rows: 0,
            subset_drop_rows: 0,
            subset_conditional_drop_rows: 0,
            ..self
        }
    }
}

thread_local! {
    static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) };
}

pub(in crate::ssa::refine) fn visit(record: impl FnOnce(&mut Counts)) {
    COUNTS.with(|counts| {
        if let Some(mut current) = counts.get() {
            record(&mut current);
            counts.set(Some(current));
        }
    });
}

pub(in crate::ssa::refine) fn observe<T>(run: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTS.with(|counts| counts.set(None));
        }
    }
    COUNTS.with(|counts| assert!(counts.replace(Some(Counts::default())).is_none()));
    let _reset = Reset;
    let result = run();
    (result, COUNTS.with(|counts| counts.take().unwrap()))
}

fn count(n: usize) -> Counts {
    let mut text = String::from("class Node() { fun touch(): Int = 1 }\n");
    for i in 0..n {
        writeln!(text, "fun run{i}(own node: Node): Unit {{ node.touch() }}").unwrap();
    }
    if n == 0 {
        text.push_str("fun neighbor(own node: Node): Unit { node.touch(); node.touch() }\n");
    }
    text.push_str("fun entry(): Unit {\n");
    if n == 0 {
        text.push_str("neighbor(Node())\n");
    }
    for i in 0..n {
        writeln!(text, "run{i}(Node())").unwrap();
    }
    text.push_str("}\n");
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "single_receiver_count.ko", &text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "entry")
        .unwrap()
        .id();
    let ((lowered, receipts), counts) = observe(|| {
        super::observe(|| {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        })
    });
    let (program, entry) = lowered.expect("reach real Unit lowering, not just AST admission");
    crate::ssa::verify::verify_program(&program).unwrap();
    #[cfg(feature = "loan-observation-test-support")]
    if n != 0 {
        use lang_frontend::ownership_checking::{
            check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
        };
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(off.ownership().loan_action_observations().is_empty());
        assert!(legacy_ownership_eq(owned.ownership(), off.ownership()));
        let entry_decl = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "entry")
            .unwrap()
            .id();
        let (((off_program, off_routes), off_receipts), off_counts) = observe(|| {
            super::observe(|| {
                crate::ssa::refine::observe_routing(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, &off, entry_decl,
                    )
                })
            })
        });
        eprintln!("OFF N={n}: {off_counts:?}");
        assert_eq!(
            off_counts.without_subset_drops(),
            Counts {
                region_rows: 0,
                drop_rows: 0,
                region_candidates: 0,
                drop_candidates: 0,
                precheck_ends: 0,
                declaration_rows: 0,
                receiver_probes: 2 * n,
                ..counts.without_subset_drops()
            }
        );
        assert!(off_receipts.is_empty());
        assert_eq!(
            off_routes
                .iter()
                .filter(|name| name.starts_with("koven.run"))
                .count(),
            n
        );
        let (off_program, off_entry) = off_program.unwrap();
        crate::ssa::verify::verify_program(&off_program).unwrap();
        assert_eq!(
            crate::ssa::render_program(&program),
            crate::ssa::render_program(&off_program)
        );
        assert_eq!(
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap(),
            crate::llvm::render_verified_program_with_entry(&off_program, off_entry).unwrap()
        );
    }
    #[cfg(not(feature = "loan-observation-test-support"))]
    let _ = entry;
    assert_eq!(
        receipts.len(),
        n,
        "one actual receipt per reachable candidate"
    );
    let callers: HashSet<_> = receipts.iter().map(|receipt| receipt.caller).collect();
    assert_eq!(callers.len(), n);
    for receipt in receipts {
        assert_eq!(receipt.ids.len(), 3, "actual Complete2 source actions");
        assert_eq!(receipt.instructions.len(), 5);
        assert_eq!(receipt.entities.len(), 3);
    }
    counts
}

/// Enforce one Unit table pass and one exact candidate inspection per real receipt.
#[test]
fn default_single_receiver_receipt_scan_work_8_and_16() {
    let n = 8;
    // The two-call neighbor shares receiver lookups but cannot publish a single-call receipt.
    assert_eq!(
        count(0).without_subset_drops(),
        Counts {
            receiver_rows: 2,
            receiver_probes: 2,
            ..Default::default()
        }
    );
    let small = count(n);
    let large = count(2 * n);
    eprintln!("N={n}: {small:?}; 2N: {large:?}");
    for (callers, counts) in [(n, small), (2 * n, large)] {
        assert_eq!(
            counts.without_subset_drops(),
            Counts {
                region_rows: 2 * callers,
                drop_rows: callers,
                region_candidates: callers,
                drop_candidates: callers,
                route_bindings: callers,
                route_ends: callers,
                route_drops: callers,
                precheck_ends: callers,
                declaration_rows: callers,
                receiver_rows: callers,
                receiver_probes: 3 * callers,
                subset_drop_index_rows: 0,
                subset_drop_rows: 0,
                subset_conditional_drop_rows: 0,
            }
        );
    }
}

/// Prior full scans visited 80/288 rows; count setup and in-scope visits separately.
#[test]
fn single_source_refine_subset_drop_prefilter_work_8_and_16() {
    for callers in [8, 16] {
        let counts = count(callers);
        eprintln!(
            "N={callers}: ordinary visits={}, conditional visits={}",
            counts.subset_drop_rows, counts.subset_conditional_drop_rows
        );
        assert_eq!(counts.subset_drop_index_rows, callers, "{counts:?}");
        assert_eq!(counts.subset_drop_rows, callers, "{counts:?}");
        assert_eq!(counts.subset_conditional_drop_rows, 0, "{counts:?}");
    }
}

#[test]
fn single_receiver_receipt_decl_lookup_preserves_cross_source_item_identity() {
    let mut sources = SourceMap::new();
    let receiver = "class Node() { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch() }\nfun probe(): Unit { run(Node()) }";
    let (left, left_unit) = unit_lower_test_support::parsed(
        &mut sources,
        "left/receiver.ko",
        &format!("package left\n{receiver}"),
    );
    let (right, right_unit) = unit_lower_test_support::parsed(
        &mut sources,
        "right/receiver.ko",
        &format!("package right\n{receiver}"),
    );
    let (main, main_unit) = unit_lower_test_support::parsed(
        &mut sources,
        "app/main.ko",
        "package app\nimport left.probe as leftProbe\nimport right.probe as rightProbe\nfun entry(): Unit { leftProbe(); rightProbe() }",
    );
    let inputs = [
        SourceUnitInput::new("root", "right/receiver.ko", right, &right_unit),
        SourceUnitInput::new("root", "app/main.ko", main, &main_unit),
        SourceUnitInput::new("root", "left/receiver.ko", left, &left_unit),
    ];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declarations = names.names().index().declarations();
    assert!(
        declarations
            .iter()
            .enumerate()
            .all(|(slot, d)| d.id().index() == slot)
    );
    let runs = declarations
        .iter()
        .filter(|d| d.name() == "run")
        .collect::<Vec<_>>();
    let [left, right] = runs.as_slice() else {
        panic!("two declarations, one per source");
    };
    assert_eq!(left.root(), right.root(), "same parsed local ItemId");
    assert_ne!(left.id(), right.id(), "different global DeclarationId");
    assert_ne!(left.source_unit(), right.source_unit());
    let entry = declarations
        .iter()
        .find(|d| d.name() == "entry")
        .unwrap()
        .id();
    let ((lowered, receipts), counts) = observe(|| {
        super::observe(|| {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        })
    });
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(counts.declaration_rows, 2);
    assert_eq!(receipts.len(), 2);
    let expected: HashSet<_> = [left, right]
        .into_iter()
        .map(|d| lang_frontend::type_checking::UnitItemId::new(d.source_unit(), d.root()))
        .collect();
    assert_eq!(
        receipts.iter().map(|r| r.callable).collect::<HashSet<_>>(),
        expected
    );
    assert_ne!(receipts[0].caller, receipts[1].caller);
    assert_eq!(receipts[0].call.expression(), receipts[1].call.expression());
    assert_ne!(receipts[0].call, receipts[1].call);
}
