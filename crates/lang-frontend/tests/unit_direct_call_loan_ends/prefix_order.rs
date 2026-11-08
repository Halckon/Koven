//! Same-point ranks come from actual producer actions, not published slice positions.
use super::*;

#[test]
fn two_auxiliary_ends_precede_source_in_actual_unwind_order() {
    let observed = observe(
        "fun select(source: String, tail: Unit): borrow String from source = source\nfun sink(first: String, second: String, stop: Unit): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, sink(\"first\", \"second\", return)) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [source] = unit.borrow_call_prefix_ends() else {
            panic!("one source")
        };
        let [second, first] = unit.direct_call_loan_ends() else {
            panic!("two actual auxiliary ends")
        };
        for (event, text, rank) in [(second, "\"second\"", 0), (first, "\"first\"", 1)] {
            assert_eq!(observed.sources.slice(event.loan().begin_span()), Ok(text));
            assert_eq!(event.point(), source.point());
            assert_eq!(event.preceding_call_loan_ends(), rank);
            assert_eq!(event.preceding_drops(), 0);
        }
        assert_eq!(source.preceding_call_loan_ends(), 2);
        assert_eq!(source.preceding_drops(), 0);
    }
}

#[test]
fn unpublished_exclusive_end_still_counts_between_observed_ends() {
    let observed = observe(
        "fun select(source: String, tail: Unit): borrow String from source = source\nfun sink(inout count: Int, value: String, stop: Unit): Unit {}\nfun run(): Unit { val owner = \"kept\"; var count = 0; borrow val item = select(owner, sink(&count, \"temporary\", return)) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [source] = unit.borrow_call_prefix_ends() else {
            panic!("one source")
        };
        let [auxiliary] = unit.direct_call_loan_ends() else {
            panic!("one published Shared end")
        };
        assert_eq!(source.point(), auxiliary.point());
        assert_eq!(auxiliary.preceding_call_loan_ends(), 0);
        // The Exclusive argument ends between these tokens, but is not in either slice.
        assert_eq!(source.preceding_call_loan_ends(), 2);
        assert_eq!(source.preceding_drops(), 0);
        assert_eq!(auxiliary.preceding_drops(), 0);
    }
}

#[test]
fn hidden_exclusive_between_shared_ends_preserves_direct_rank_gap() {
    let observed = observe(
        "fun select(source: String, tail: Unit): borrow String from source = source\nfun sink(first: String, inout count: Int, last: String, stop: Unit): Unit {}\nfun run(): Unit { val owner = \"kept\"; var count = 0; borrow val item = select(owner, sink(\"first\", &count, \"last\", return)) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [source] = unit.borrow_call_prefix_ends() else {
            panic!("one source")
        };
        let [last, first] = unit.direct_call_loan_ends() else {
            panic!("two Shared ends with an unpublished Exclusive end between")
        };
        for (event, text, rank) in [(last, "\"last\"", 0), (first, "\"first\"", 2)] {
            assert_eq!(observed.sources.slice(event.loan().begin_span()), Ok(text));
            assert_eq!(event.point(), source.point());
            assert_eq!(event.preceding_call_loan_ends(), rank);
            assert_eq!(event.preceding_drops(), 0);
        }
        assert_eq!(source.preceding_call_loan_ends(), 3);
        assert_eq!(source.preceding_drops(), 0);
    }
}

#[test]
fn completed_nested_call_is_not_replayed_when_later_named_operands_unwind() {
    let observed = observe(
        "fun ready(text: String): Int = 0\nfun sink(first: String, second: String, stop: Unit): Unit {}\nfun select(source: String, own done: Int, tail: Unit): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(source = owner, done = ready(owner), tail = sink(second = owner, first = owner, stop = return)) }",
    );
    assert_units_admitted(&observed);
    // All four loans share a root. Call/argument identity, not that root or
    // parameter order, determines which completed token is removed and which
    // pending tokens unwind. These bytes come from the literal source above.
    assert_fixed_ends(
        &observed,
        &[
            ExpectedEnd {
                call: (270, 282),
                argument: (276, 281),
                parameter: (10, 22),
                point: ExpectedPoint::CallReturn,
                target: ExpectedTarget::Local((206, 211)),
            },
            ExpectedEnd {
                call: (291, 341),
                argument: (320, 325),
                parameter: (42, 55),
                point: ExpectedPoint::Return((334, 340)),
                target: ExpectedTarget::Local((206, 211)),
            },
            ExpectedEnd {
                call: (291, 341),
                argument: (305, 310),
                parameter: (57, 71),
                point: ExpectedPoint::Return((334, 340)),
                target: ExpectedTarget::Local((206, 211)),
            },
        ],
    );
    super::prefix_oracle::assert_prefix_identities(
        &observed,
        &[((240, 342), (256, 261), (105, 119), (206, 211), (334, 340))],
    );
    for unit in &observed.units {
        let [completed, first, second] = unit.direct_call_loan_ends() else {
            panic!("one completed token and two pending tokens, each exactly once")
        };
        let [source] = unit.borrow_call_prefix_ends() else {
            panic!("one outer source unwind")
        };
        assert_ne!(completed.point(), source.point());
        assert_eq!(completed.preceding_call_loan_ends(), 0);
        for (end, rank) in [(first, 0), (second, 1)] {
            assert_eq!(end.point(), source.point());
            assert_eq!(end.preceding_call_loan_ends(), rank);
            assert_eq!(end.loan().target(), source.loan().target());
        }
        assert_eq!(completed.loan().target(), source.loan().target());
        assert_eq!(source.preceding_call_loan_ends(), 2);
        let UnitLoanTarget::Place(place) = source.loan().target() else {
            panic!("owned source root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), source.point());
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().ends().is_empty());
        assert!(unit.borrow_call_handoffs().is_empty());
    }
}
