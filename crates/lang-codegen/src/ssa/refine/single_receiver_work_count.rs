//! Test-only row counts. These are neither timings nor full compiler complexity.
use crate::ssa::unit_lower_test_support;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
use std::{cell::Cell, collections::HashSet, fmt::Write};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ssa::refine) struct Counts {
    pub(in crate::ssa::refine) region_rows: usize,
    pub(in crate::ssa::refine) drop_rows: usize,
    pub(in crate::ssa::refine) region_candidates: usize,
    pub(in crate::ssa::refine) drop_candidates: usize,
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

fn observe<T>(run: impl FnOnce() -> T) -> (T, Counts) {
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
        assert_eq!(off_counts, Counts::default());
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
    assert_eq!(count(0), Counts::default());
    let small = count(n);
    let large = count(2 * n);
    eprintln!("N={n}: {small:?}; 2N: {large:?}");
    for (callers, counts) in [(n, small), (2 * n, large)] {
        assert_eq!(
            counts,
            Counts {
                region_rows: 2 * callers,
                drop_rows: callers,
                region_candidates: callers,
                drop_candidates: callers,
            }
        );
    }
}
