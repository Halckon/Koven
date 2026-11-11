//! CI-only corruptions of real producer receipts; compile locally, never execute locally.
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
thread_local! {static SNAPSHOTS:RefCell<Option<Vec<Snapshot>>>=const{RefCell::new(None)};}
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
fn capture_topology(mirror: bool) -> Snapshot {
    let text = "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) } }\nfun consume(s: String): Unit {}\nfun run(a: Boolean, b: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner,a,b); consume(item) }";
    let text = if mirror {
        text.replace("if (a) { if (b) { return left(s) } else { return middle(s) } } else { return right(s) }", "if (a) { return left(s) } else { if (b) { return middle(s) } else { return right(s) } }")
    } else {
        text.to_owned()
    };
    capture_text(&text)
}
fn capture_text(text: &str) -> Snapshot {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "nested-ci.ko", text);
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
#[test]
fn ci_only_nested_receipt_rejects_foreign_ids_missing_parent_and_wrong_flag() {
    let s = capture();
    let foreign = capture();
    assert_ne!(s.receipt.caller, foreign.receipt.caller);
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..13 {
        let mut r = s.receipt.clone();
        match fault {
            0 => {
                r.paths[0].edges.remove(0);
            }
            1 => r.paths[0].edges[0] = r.splits[0].edges[1].clone(),
            2 => r.splits[1].flag = r.source,
            3 => r.paths[0].callee = r.paths[1].callee,
            4 => r.paths[0].ids = foreign.receipt.paths[0].ids.clone(),
            5 => r.splits[0].read = foreign.receipt.splits[0].read,
            6 => {
                r.paths.pop();
            }
            7 => {
                r.splits.pop();
            }
            8 => r.paths[0].result = r.paths[1].result,
            9 => r.paths[0].source = r.paths[1].source,
            10 => r.splits[1].edges.swap(0, 1),
            11 => r.caller = foreign.receipt.caller,
            _ => r.paths[0].control = r.paths[1].control,
        }
        assert!(
            r.validate(&s.plans, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
    for fault in 0..4 {
        let mut p = s.plans.clone();
        let plan = p.plan.as_mut().unwrap();
        match fault {
            0 => plan.splits[1].join = None,
            1 => plan.splits[0].join = None,
            2 => plan.paths[2].stage = 2,
            _ => plan.splits[1].done = false,
        }
        assert!(s.receipt.validate(&p, &s.function, s.span).is_err());
    }
}
#[test]
fn ci_only_nested_receipt_publication_rolls_back_before_original_verifier_success() {
    let s = capture();
    let (_, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
        RECEIPTS.with(|v| {
            v.borrow_mut()
                .as_mut()
                .unwrap()
                .pending
                .push(s.receipt.clone())
        });
        Err::<(), ()>(())
    });
    assert!(receipts.is_empty());
    let (_, receipts) = observe(|| {
        let guard = UnitPublication::begin();
        s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
        RECEIPTS.with(|v| {
            v.borrow_mut()
                .as_mut()
                .unwrap()
                .pending
                .push(s.receipt.clone())
        });
        guard.commit();
    });
    assert_eq!(receipts.len(), 1);
}

#[test]
fn ci_only_mirror_receipt_rejects_wrong_topology_missing_false_parent_and_flag() {
    let s = capture_topology(true);
    assert_eq!(
        s.receipt.topology,
        UnitBorrowNestedForwardTopology::NestedElse
    );
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..5 {
        let mut r = s.receipt.clone();
        match fault {
            0 => r.topology = UnitBorrowNestedForwardTopology::NestedThen,
            1 => {
                r.paths[1].edges.remove(0);
            }
            2 => r.paths[1].edges[0] = r.splits[0].edges[0].clone(),
            3 => r.splits[1].flag = r.source,
            _ => r.paths[2].edges[1] = r.splits[1].edges[0].clone(),
        }
        assert!(
            r.validate(&s.plans, &s.function, s.span).is_err(),
            "mirror fault {fault}"
        );
    }
}
#[test]
fn ci_only_mirror_producer_requires_plain_leaf_return_before_inner_split() {
    let s = capture_topology(true);
    for stage in 0..=3 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        let inner = p.splits[1].clone();
        let borrowed = BTreeMap::from([
            (p.root, p.source.unwrap()),
            (p.flags[1], inner.flag.unwrap()),
        ]);
        p.paths[0].stage = stage;
        p.paths[1].stage = 0;
        p.paths[2].stage = 0;
        p.splits[1].entry = None;
        p.splits[1].arms = None;
        p.splits[1].flag = None;
        p.splits[1].value = None;
        p.splits[1].done = false;
        p.splits[1].join = None;
        p.splits[0].join = None;
        let result = plans.split(
            inner.split,
            inner.condition,
            inner.entry.unwrap(),
            inner.arms.unwrap(),
            &borrowed,
            s.span,
        );
        assert_eq!(
            result.is_ok(),
            stage == 3,
            "actual producer precondition stage {stage}"
        );
    }
}

fn capture_balanced() -> Snapshot {
    capture_text(
        "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun last(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } } else { if (c) { return right(s) } else { return last(s) } } }\nfun consume(s: String): Unit {}\nfun run(a: Boolean, b: Boolean, c: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner,a,b,c); consume(item) }",
    )
}
#[test]
fn ci_only_balanced_receipt_rejects_cross_subtree_identity_and_incomplete_closure() {
    let s = capture_balanced();
    let foreign = capture_balanced();
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..8 {
        let mut r = s.receipt.clone();
        match fault {
            0 => r.topology = UnitBorrowNestedForwardTopology::NestedThen,
            1 => {
                r.paths[2].edges.remove(0);
            }
            2 => r.paths[2].edges[0] = r.splits[0].edges[0].clone(),
            3 => r.splits[2].flag = r.splits[1].flag,
            4 => r.paths[3].callee = r.paths[0].callee,
            5 => r.paths[2].ids = foreign.receipt.paths[2].ids.clone(),
            6 => r.splits[2].read = foreign.receipt.splits[2].read,
            _ => {
                r.paths.pop();
            }
        }
        assert!(
            r.validate(&s.plans, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
    for split in [1, 2] {
        let mut bad = s.plans.clone();
        bad.plan.as_mut().unwrap().splits[split].join = None;
        assert!(s.receipt.validate(&bad, &s.function, s.span).is_err());
    }
}
#[test]
fn ci_only_balanced_producer_requires_left_returns_and_join_before_right_split() {
    let s = capture_balanced();
    for fault in 0..4 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        let entry = p.splits[2].entry.unwrap();
        let arms = p.splits[2].arms.unwrap();
        let control = p.splits[2].split;
        let condition = p.splits[2].condition;
        let borrowed = BTreeMap::from([
            (p.root, p.source.unwrap()),
            (p.flags[2], p.splits[2].flag.unwrap()),
        ]);
        p.splits[2].entry = None;
        p.splits[2].join = None;
        p.splits[2].done = false;
        for path in &mut p.paths[2..] {
            path.stage = 0;
        }
        match fault {
            0 => p.paths[0].stage = 2,
            1 => p.paths[1].stage = 2,
            2 => p.splits[1].join = None,
            _ => {}
        }
        let result = plans.split(control, condition, entry, arms, &borrowed, s.span);
        assert_eq!(result.is_ok(), fault == 3);
    }
}

#[test]
fn ci_only_shared_inner_keeps_binding_and_both_parent_edges() {
    let text = "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun last(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } } else { if (b) { return right(s) } else { return last(s) } } }\nfun consume(s: String): Unit {}\nfun run(a: Boolean, b: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner,a,b); consume(item) }";
    let s = capture_text(text);
    assert_eq!(
        s.receipt.flag_layout,
        UnitBorrowNestedForwardFlagLayout::SharedInner
    );
    assert_ne!(s.receipt.splits[1].flag, s.receipt.splits[2].flag);
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..6 {
        let mut r = s.receipt.clone();
        match fault {
            0 => r.flag_layout = UnitBorrowNestedForwardFlagLayout::Distinct,
            1 => r.splits[2].flag = r.splits[1].flag,
            2 => r.splits[2].flag = r.splits[0].flag,
            3 => r.splits[2].condition = r.splits[1].condition,
            4 => r.splits[2].read = r.splits[1].read,
            _ => r.paths[2].edges[0] = r.paths[0].edges[0].clone(),
        }
        assert!(
            r.validate(&s.plans, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
    for fault in 0..3 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        match fault {
            0 => p.flag_layout = UnitBorrowNestedForwardFlagLayout::Distinct,
            1 => p.entry_flags[1] = p.entry_flags[0],
            _ => p.flags[2] = p.flags[0],
        }
        assert!(s.receipt.validate(&plans, &s.function, s.span).is_err());
    }
}
