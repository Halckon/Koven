//! CI-only corruptions of real producer receipts; compile locally, never execute locally.
use super::*;
use crate::ssa::unit_lower_test_support;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
use std::cell::RefCell;
#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) plans: Plans,
    pub(super) function: Function,
    pub(super) receipt: Receipt,
    pub(super) span: Span,
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
    capture_text(
        "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun fourth(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { return left(s) } else { if (b) { return middle(s) } else { if (c) { return right(s) } } }; return fourth(s) }\nfun consume(s: String): Unit {}\nfun run(a: Boolean, b: Boolean,c: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner,a,b,c); consume(item) }",
    )
}
pub(super) fn capture_text(text: &str) -> Snapshot {
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
fn ci_only_single_receipt_rejects_foreign_ids_missing_parent_and_wrong_flag() {
    let s = capture();
    let foreign = capture();
    assert_ne!(s.receipt.caller, foreign.receipt.caller);
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..16 {
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
            12 => r.paths[0].control = r.paths[1].control,
            13 => r.splits[2].flag = r.splits[1].flag,
            14 => r.splits[0].edges[1].arguments.swap(1, 2),
            _ => r.paths[0].ids.clear(),
        }
        assert!(
            r.validate(&s.plans, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
    for fault in 0..7 {
        let mut p = s.plans.clone();
        let plan = p.plan.as_mut().unwrap();
        match fault {
            0 => plan.splits[1].join = None,
            1 => plan.splits[0].join = None,
            2 => plan.paths[2].stage = 2,
            3 => plan.splits[1].done = false,
            4 => plan.splits[2].parent = Some((2, 0)),
            5 => {
                plan.paths.pop();
            }
            _ => plan.cursor -= 1,
        }
        assert!(s.receipt.validate(&p, &s.function, s.span).is_err());
    }
}
#[test]
fn ci_only_single_receipt_publication_rolls_back_before_original_verifier_success() {
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
fn ci_only_single_original_gotos_folds_and_tail_identity_are_mandatory() {
    let s = capture();
    assert!(s.receipt.folded_continuation_chain());
    for fault in 0..7 {
        let mut receipt = s.receipt.clone();
        match fault {
            0 => {
                receipt.original_gotos.pop();
            }
            1 => {
                receipt.folds.pop();
            }
            2 => {
                receipt.original_gotos.swap(0, 1);
            }
            3 => {
                receipt.folds.push(receipt.folds[0]);
            }
            4 => receipt.original_tail = receipt.retained_hole,
            5 => receipt.final_tail = receipt.paths[0].block,
            _ => receipt.folds[0] = (receipt.retained_hole, receipt.original_tail),
        }
        assert!(
            receipt.validate(&s.plans, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
    for fault in 0..5 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        match fault {
            0 => {
                p.gotos.pop();
            }
            1 => {
                p.gotos[0].0 = p.gotos[0].1;
            }
            2 => {
                p.hole_route.last_mut().unwrap().1 = 0;
            }
            3 => {
                p.paths.last_mut().unwrap().block = Some(s.receipt.retained_hole);
            }
            _ => {
                plans.folds.push(plans.folds[0]);
            }
        }
        assert!(s.receipt.validate(&plans, &s.function, s.span).is_err());
    }
}

#[test]
fn ci_only_single_live_join_and_goto_hooks_reject_wrong_predecessors() {
    use super::super::tree::Event;
    let s = capture();
    let baseline = s.plans.plan.as_ref().unwrap();
    let split = baseline.hole_route.last().unwrap().0;
    let (from, to) = baseline.gotos[0];
    for pred in [vec![], vec![to], vec![from, from]] {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        p.cursor = p
            .events()
            .unwrap()
            .iter()
            .position(|e| *e == Event::Join(split))
            .unwrap();
        p.splits[split].join = None;
        let control = p.splits[split].split;
        assert!(plans.join(control, to, &pred, s.span).is_err());
    }
    for (block, target) in [(to, to), (from, from), (from, baseline.gotos[1].1)] {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        p.cursor = p
            .events()
            .unwrap()
            .iter()
            .position(|e| *e == Event::Goto(split))
            .unwrap();
        p.gotos.clear();
        assert!(
            plans
                .terminate(&RTerm::Goto(target), block, s.span)
                .is_err()
        );
    }
    let mut plans = s.plans.clone();
    plans.folds.clear();
    assert!(
        plans
            .fold(s.receipt.retained_hole, s.receipt.original_tail, s.span)
            .is_err()
    );
    for pair in &s.receipt.folds {
        plans.fold(pair.0, pair.1, s.span).unwrap();
    }
    let pair = s.receipt.folds[0];
    assert!(plans.fold(pair.0, pair.1, s.span).is_err());
}
