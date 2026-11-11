//! CI-only mutations of real producer snapshots. Compile locally; never run locally.
use super::*;
use crate::ssa::unit_lower_test_support;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
use std::cell::RefCell;

#[derive(Clone)]
struct Snapshot {
    plans: Plans,
    function: Function,
    receipt: Receipt,
    span: Span,
}
thread_local! { static SNAPSHOTS: RefCell<Option<Vec<Snapshot>>> = const { RefCell::new(None) }; }
pub(super) fn record(plans: &Plans, function: &Function, receipt: &Receipt, span: Span) {
    SNAPSHOTS.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            v.push(Snapshot {
                plans: plans.clone(),
                function: function.clone(),
                receipt: receipt.clone(),
                span,
            });
        }
    });
}
fn capture() -> Snapshot {
    capture_topology(false)
}
fn capture_topology(both: bool) -> Snapshot {
    let text = "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view(source) }; return view(source) }\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, flag); consume(item) }";
    let text = if both {
        text.replace("}; return view(source)", "} else { return view(source) }")
    } else {
        text.to_owned()
    };
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "ci-branch.ko", &text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
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
    SNAPSHOTS.with(|v| *v.borrow_mut() = Some(Vec::new()));
    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources, &inputs, &names, &types, &typed, &owned, entry,
    )
    .unwrap();
    let mut snapshots = SNAPSHOTS.with(|v| v.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    snapshots.pop().unwrap()
}
fn completed(s: &Snapshot) -> Emitted {
    let r = &s.receipt;
    Emitted {
        layout: Some(Layout {
            entry: r.entry,
            source: r.source,
            flag: r.flag,
            arms: [r.paths[0].block, r.paths[1].block],
            sources: [r.paths[0].source, r.paths[1].source],
        }),
        read: Some((r.read, r.condition_value)),
        calls: [
            Some((r.paths[0].instruction, r.paths[0].result)),
            Some((r.paths[1].instruction, r.paths[1].result)),
        ],
        edges: Some([r.paths[0].edge.clone(), r.paths[1].edge.clone()]),
        returned: [true; 2],
    }
}
#[test]
fn ci_only_branch_receipt_rejects_corrupt_identity_and_incomplete_paths() {
    let s = capture();
    let foreign = capture();
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..12 {
        let mut r = s.receipt.clone();
        match fault {
            0 => r.paths.pop().map(|_| ()).unwrap(),
            1 => r.paths[1] = r.paths[0].clone(),
            2 => r.paths[0].edge.arguments = vec![EntityId::Loan(r.flag)],
            3 => r.paths[0].edge.target = r.paths[1].block,
            4 => r.paths[0].source = r.flag,
            5 => r.paths[0].result = r.paths[1].result,
            6 => r.paths[0].instruction = foreign.receipt.paths[0].instruction,
            7 => r.paths[0].ids = foreign.receipt.paths[0].ids.clone(),
            8 => r.read = r.paths[0].instruction,
            9 => r.condition_value = foreign.receipt.condition_value,
            10 => r.fold = r.fold.map(|(from, to)| (to, from)),
            _ => r.caller = foreign.receipt.caller,
        }
        assert!(
            r.validate(&s.plans, &s.function, s.span).is_err(),
            "identity fault {fault}"
        );
    }
    for fault in 0..6 {
        let mut e = completed(&s);
        match fault {
            0 => e.calls[0] = None,
            1 => e.calls[1] = None,
            2 => e.returned[0] = false,
            3 => e.returned[1] = false,
            4 => e.edges = None,
            _ => e.read = None,
        }
        assert!(
            e.finish(&s.plans, &s.function, s.span).is_err(),
            "missing role {fault}"
        );
    }
    let mut plans = s.plans.clone();
    plans.folds.clear();
    let (block, successor) = s.receipt.fold.unwrap();
    assert!(plans.fold(successor, block, s.span).is_err());
    plans.fold(block, successor, s.span).unwrap();
    assert!(plans.fold(block, successor, s.span).is_err());
    for fault in 0..3 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        match fault {
            0 => p.goto_done = false,
            1 => p.paths[1].stage = 2,
            _ => p.split_done = false,
        }
        assert!(Emitted::new(&plans, &s.function, s.span).is_err());
    }
}

#[test]
fn ci_only_branch_receipt_publication_is_atomic_on_unit_failure() {
    let s = capture();
    let (_, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        completed(&s).finish(&s.plans, &s.function, s.span).unwrap();
        // Simulate a later lowering/verifier error: successful function emission stays pending.
        Err::<(), ()>(())
    });
    assert!(receipts.is_empty());
    let (_, receipts) = observe(|| {
        let guard = UnitPublication::begin();
        completed(&s).finish(&s.plans, &s.function, s.span).unwrap();
        guard.commit();
    });
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].paths.len(), 2);
}

#[test]
fn ci_only_both_return_receipt_rejects_tail_topology_and_missing_exit() {
    let s = capture_topology(true);
    assert_eq!(
        s.receipt.topology,
        UnitBorrowBranchForwardTopology::BothArmsReturn
    );
    assert!(s.receipt.fold.is_none());
    assert!(s.plans.folds.is_empty());
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..5 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        match fault {
            0 => p.topology = UnitBorrowBranchForwardTopology::TailContinuation,
            1 => p.goto_done = true,
            2 => p.paths[1].stage = 2,
            3 => p.join = None,
            _ => plans.folds.push((p.arms.unwrap()[1], p.join.unwrap())),
        }
        assert!(Emitted::new(&plans, &s.function, s.span).is_err());
    }
    for fault in 0..3 {
        let mut r = s.receipt.clone();
        match fault {
            0 => r.topology = UnitBorrowBranchForwardTopology::TailContinuation,
            1 => r.fold = Some((BlockRef(1), BlockRef(2))),
            _ => {
                r.paths.pop();
            }
        }
        assert!(r.validate(&s.plans, &s.function, s.span).is_err());
    }
    let mut plans = s.plans.clone();
    let p = plans.plan.as_ref().unwrap();
    let arm = p.arms.unwrap()[1];
    let join = p.join.unwrap();
    assert!(plans.fold(arm, join, s.span).is_err());
    for fault in 0..2 {
        let mut e = completed(&s);
        e.returned[fault] = false;
        assert!(e.finish(&s.plans, &s.function, s.span).is_err());
    }
}
