//! CI-only faults of the real tail producer. Compile locally; never execute locally.
use super::ci_tests::capture_text;
use super::*;

const SOURCE: &str = "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { if (b) { return left(s) } else { return middle(s) } }\nreturn right(s) }\nfun consume(s: String): Unit {}\nfun run(a: Boolean, b: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner,a,b); consume(item) }";

#[test]
fn ci_only_tail_receipt_rejects_fold_and_continuation_identity_substitution() {
    let s = capture_text(SOURCE);
    let foreign = capture_text(SOURCE);
    assert_ne!(s.receipt.caller, foreign.receipt.caller);
    assert!(s.receipt.folded_distinct_blocks());
    s.receipt.validate(&s.plans, &s.function, s.span).unwrap();
    for fault in 0..10 {
        let mut receipt = s.receipt.clone();
        match fault {
            0 => receipt.fold = None,
            1 => receipt.fold = receipt.fold.map(|(from, to)| (to, from)),
            2 => receipt.paths[2].edges[0] = receipt.splits[0].edges[0].clone(),
            3 => receipt.paths[2].source = receipt.paths[0].source,
            4 => receipt.paths[2].result = receipt.paths[0].result,
            5 => receipt.paths[2].instruction = foreign.receipt.paths[2].instruction,
            6 => receipt.paths[2].ids = foreign.receipt.paths[2].ids.clone(),
            7 => receipt.topology = UnitBorrowNestedForwardTopology::NestedThen,
            8 => {
                receipt.paths[2].edges.clear();
            }
            _ => receipt.paths[2].block = receipt.paths[0].block,
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
            0 => p.goto_done = false,
            1 => p.splits[1].join = None,
            2 => p.splits[0].join = None,
            3 => p.paths[2].stage = 2,
            _ => plans.folds.clear(),
        }
        assert!(
            s.receipt.validate(&plans, &s.function, s.span).is_err(),
            "missing proof {fault}"
        );
    }
}

#[test]
fn ci_only_tail_fold_is_unique_and_has_exact_producer_endpoints() {
    let s = capture_text(SOURCE);
    let (from, join) = s.receipt.fold.unwrap();
    let mut plans = s.plans.clone();
    plans.folds.clear();
    assert!(plans.fold(join, from, s.span).is_err());
    plans.fold(from, join, s.span).unwrap();
    assert!(plans.fold(from, join, s.span).is_err());
    for fault in 0..3 {
        let mut plans = s.plans.clone();
        let p = plans.plan.as_mut().unwrap();
        let control = p.splits[0].split;
        let arms = p.splits[0].arms.unwrap();
        p.splits[0].join = None;
        p.paths[2].stage = 0;
        p.goto_done = false;
        let predecessors = match fault {
            0 => vec![],
            1 => vec![arms[0]],
            _ => vec![arms[0], arms[1]],
        };
        assert!(
            plans.join(control, join, &predecessors, s.span).is_err(),
            "predecessor fault {fault}"
        );
    }
}

#[test]
fn ci_only_tail_publication_does_not_escape_failed_unit() {
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
