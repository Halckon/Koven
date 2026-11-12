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
        "fun left(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun tail(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean, c: Boolean): borrow String from s { if (a) { if (b) { return left(s) } } else { if (c) { return right(s) } }; return tail(s) }\nfun consume(s: String): Unit {}\nfun run(a: Boolean,b: Boolean,c: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner,a,b,c); consume(item) }",
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
fn ci_only_multi_receipt_rejects_foreign_ids_missing_parent_and_wrong_flag() {
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
            14 => r.splits[0].edges[1].arguments[1] = EntityId::Loan(r.source),
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
fn ci_only_multi_receipt_publication_rolls_back_before_original_verifier_success() {
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
fn ci_only_multi_shared_tail_requires_exact_original_graph_and_actual_paths() {
    let s = capture();
    assert_eq!(s.receipt.origins.len(), 2);
    assert_eq!(s.receipt.continuation_edges.len(), 2);
    for fault in 0..15 {
        let mut r = s.receipt.clone();
        match fault {
            0 => {
                r.original_gotos.pop();
            }
            1 => {
                r.folds.pop();
            }
            2 => r.original_gotos.swap(0, 1),
            3 => r.folds.push(r.folds[0]),
            4 => r.original_tail = r.original_gotos[0].0,
            5 => r.final_tail = r.paths[0].block,
            6 => {
                r.continuation_edges.pop();
            }
            7 => r.continuation_edges[0].1 = r.continuation_edges[1].1.clone(),
            8 => r.continuation_edges[0].0 = r.continuation_edges[1].0,
            9 => {
                r.origins.pop();
            }
            10 => r.origins[0] = r.origins[1].clone(),
            11 => {
                r.origins[0].pop();
            }
            12 => r.paths.last_mut().unwrap().source = r.paths[0].source,
            13 => r.paths.push(r.paths.last().unwrap().clone()),
            _ => {
                let k = *r.blocks.keys().next().unwrap();
                r.blocks.remove(&k);
            }
        }
        assert!(
            r.validate(&s.plans, &s.function, s.span).is_err(),
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
            1 => p.gotos.push(p.gotos[0]),
            2 => p.hole_routes[0] = p.hole_routes[1].clone(),
            3 => p.paths.last_mut().unwrap().block = p.splits[1].arms.map(|a| a[1]),
            _ => {
                plans.folds.pop();
            }
        }
        assert!(
            s.receipt.validate(&plans, &s.function, s.span).is_err(),
            "plan fault {fault}"
        );
    }
}
#[test]
fn ci_only_multi_join_and_fold_require_two_real_predecessors() {
    use super::super::tree::Event;
    let s = capture();
    let baseline = s.plans.plan.as_ref().unwrap();
    let join = baseline.splits[0].join.unwrap();
    let pred = [
        baseline.predecessor(0, 0).unwrap(),
        baseline.predecessor(0, 1).unwrap(),
    ];
    let reset = || {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        p.cursor = p
            .events()
            .unwrap()
            .iter()
            .position(|e| *e == Event::Join(0))
            .unwrap();
        p.splits[0].join = None;
        plans
    };
    reset()
        .join(baseline.splits[0].split, join, &pred, s.span)
        .unwrap();
    for wrong in [
        vec![],
        vec![pred[0]],
        vec![pred[1], pred[0]],
        vec![pred[0], pred[0]],
    ] {
        assert!(
            reset()
                .join(baseline.splits[0].split, join, &wrong, s.span)
                .is_err()
        );
    }
    let mut p = s.plans.clone();
    p.folds.clear();
    // A genuine Goto targeting this two-predecessor join is not fold authority.
    assert!(p.fold(pred[0], join, s.span).is_err());
    for (from, to) in &s.receipt.folds {
        p.fold(*from, *to, s.span).unwrap();
    }
    let (from, to) = p.remaining_edges().unwrap()[0];
    assert!(p.fold(from, to, s.span).is_err());
    let (from, to) = s.receipt.folds[0];
    assert!(p.fold(from, to, s.span).is_err());
}

#[test]
fn ci_only_multi_original_goto_hook_is_not_replaced_by_final_cfg() {
    use super::super::tree::Event;
    let s = capture();
    let baseline = s.plans.plan.as_ref().unwrap();
    let from = baseline.predecessor(0, 0).unwrap();
    let to = baseline.splits[0].join.unwrap();
    let reset = || {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        let events = p.events().unwrap();
        p.cursor = events.iter().position(|e| *e == Event::Goto(0, 0)).unwrap();
        let preceding = events[..p.cursor]
            .iter()
            .filter(|e| matches!(e, Event::Goto(_, _)))
            .count();
        p.gotos.truncate(preceding);
        plans
    };
    reset().terminate(&RTerm::Goto(to), from, s.span).unwrap();
    for (wrong_from, wrong_to) in [
        (to, to),
        (from, from),
        (baseline.predecessor(0, 1).unwrap(), to),
    ] {
        assert!(
            reset()
                .terminate(&RTerm::Goto(wrong_to), wrong_from, s.span)
                .is_err()
        );
    }
}
