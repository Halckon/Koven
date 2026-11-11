//! CI-only double-Goto/double-fold producer faults. Compile locally, never execute locally.
use super::ci_tests::capture_text;
use super::*;

const SOURCE: &str = "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { return left(s) } else { if (b) { return middle(s) } }\nreturn right(s) }\nfun consume(s: String): Unit {}\nfun run(a: Boolean, b: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner,a,b); consume(item) }";

#[test]
fn ci_only_cascade_receipt_rejects_replaced_origins_and_final_ssa() {
    let s = capture_text(SOURCE);
    let foreign = capture_text(SOURCE);
    assert_ne!(s.receipt.caller, foreign.receipt.caller);
    assert!(s.receipt.cascade_folded_distinct_blocks());
    assert!(s.receipt.fold.is_none());
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..13 {
        let mut receipt = s.receipt.clone();
        match fault {
            0 => receipt.cascade_folds.clear(),
            1 => {
                receipt.cascade_folds.pop();
            }
            2 => receipt.cascade_folds.swap(0, 1),
            3 => receipt.cascade_folds[1].0 = receipt.cascade_folds[0].1,
            4 => receipt.fold = Some(receipt.cascade_folds[0]),
            5 => receipt.paths[2].edges[0] = receipt.splits[0].edges[0].clone(),
            6 => receipt.paths[2].edges[1] = receipt.splits[1].edges[0].clone(),
            7 => receipt.paths[2].source = receipt.paths[1].source,
            8 => receipt.paths[2].result = receipt.paths[1].result,
            9 => receipt.paths[2].instruction = foreign.receipt.paths[2].instruction,
            10 => receipt.paths[2].ids = foreign.receipt.paths[2].ids.clone(),
            11 => receipt.topology = UnitBorrowNestedForwardTopology::NestedElse,
            _ => receipt.paths[2].block = receipt.paths[1].block,
        }
        assert!(
            receipt.validate(&s.plans, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
    for fault in 0..8 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        match fault {
            0 => p.cascade_gotos[0] = false,
            1 => p.cascade_gotos[1] = false,
            2 => p.splits[1].join = None,
            3 => p.splits[0].join = p.splits[1].join,
            4 => p.paths[2].block = Some(p.splits[1].arms.unwrap()[1]),
            5 => p.goto_done = true,
            6 => plans.folds.swap(0, 1),
            _ => plans.folds[1].0 = plans.folds[0].1,
        }
        assert!(
            s.receipt.validate(&plans, &s.function, s.span).is_err(),
            "missing origin {fault}"
        );
    }
}

#[test]
fn ci_only_cascade_fold_order_preserves_original_joins_and_rewritten_predecessor() {
    let s = capture_text(SOURCE);
    let p = s.plans.plan.as_ref().unwrap();
    let inner_false = p.splits[1].arms.unwrap()[1];
    let inner_join = p.splits[1].join.unwrap();
    let outer_join = p.splits[0].join.unwrap();
    assert_ne!(inner_false, inner_join);
    assert_ne!(inner_false, outer_join);
    assert_ne!(inner_join, outer_join);
    assert_eq!(p.paths[2].block, Some(outer_join));
    assert_eq!(
        s.receipt.cascade_folds,
        [(inner_false, inner_join), (inner_false, outer_join)]
    );
    let mut plans = s.plans.clone();
    plans.folds.clear();
    assert!(plans.fold(inner_false, outer_join, s.span).is_err());
    plans.fold(inner_false, inner_join, s.span).unwrap();
    // The second original Goto originated at inner_join, but the actual fold no longer does.
    assert!(plans.fold(inner_join, outer_join, s.span).is_err());
    plans.fold(inner_false, outer_join, s.span).unwrap();
    assert!(plans.fold(inner_false, outer_join, s.span).is_err());
}

#[test]
fn ci_only_cascade_original_gotos_cannot_be_replaced_by_folded_predecessors() {
    let s = capture_text(SOURCE);
    let p = s.plans.plan.as_ref().unwrap();
    let inner_false = p.splits[1].arms.unwrap()[1];
    let inner_join = p.splits[1].join.unwrap();
    let outer_join = p.splits[0].join.unwrap();
    for fault in 0..4 {
        let mut plans = s.plans.clone();
        plans.folds.clear();
        let p = plans.plan.as_mut().unwrap();
        p.paths[2].stage = 0;
        p.cascade_gotos = [fault != 0, false];
        let (block, target) = match fault {
            0 => (inner_join, outer_join),  // Missing first Goto.
            1 => (inner_false, outer_join), // Final folded predecessor is not original producer.
            2 => (inner_join, inner_join),
            _ => (inner_join, outer_join),
        };
        let result = plans.terminate(&RTerm::Goto(target), block, s.span);
        assert_eq!(result.is_ok(), fault == 3, "fault {fault}");
        assert!(
            plans
                .terminate(&RTerm::Goto(target), block, s.span)
                .is_err()
        );
    }
}

#[test]
fn ci_only_cascade_receipt_publication_rolls_back_with_failed_unit() {
    let s = capture_text(SOURCE);
    let (_, published) = observe(|| {
        let _guard = UnitPublication::begin();
        s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
        RECEIPTS.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .unwrap()
                .pending
                .push(s.receipt.clone())
        });
        Err::<(), ()>(())
    });
    assert!(published.is_empty());
}
