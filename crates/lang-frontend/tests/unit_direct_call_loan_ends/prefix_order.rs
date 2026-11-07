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
