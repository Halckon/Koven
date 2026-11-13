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

#[test]
fn sibling_return_ranks_count_only_their_own_established_auxiliary_loans() {
    let observed = observe(
        "fun select(source: String, tail: Unit): borrow String from source = source\nfun one(value: String, stop: Unit): Unit {}\nfun two(first: String, second: String, stop: Unit): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = select(owner, if (flag) { one(\"left\", return) } else { two(\"right-first\", \"right-second\", return) }) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let sources = unit.borrow_call_prefix_ends();
        assert_eq!(sources.len(), 2, "one source unwind per Return edge");
        assert_ne!(sources[0].point(), sources[1].point());
        assert_eq!(
            sources[0].loan(),
            sources[1].loan(),
            "same pending outer token"
        );
        assert_eq!(unit.direct_call_loan_ends().len(), 3);
        let mut paths = Vec::new();
        for source in sources {
            assert!(unit.loans().contains(source.loan()));
            let auxiliary: Vec<_> = unit
                .direct_call_loan_ends()
                .iter()
                .filter(|end| end.point() == source.point())
                .collect();
            let texts: Vec<_> = auxiliary
                .iter()
                .map(|end| observed.sources.slice(end.loan().begin_span()).unwrap())
                .collect();
            let expected = match texts.as_slice() {
                ["\"left\""] => 1,
                ["\"right-second\"", "\"right-first\""] => 2,
                other => panic!("unexpected branch unwind: {other:?}"),
            };
            for (rank, end) in auxiliary.iter().enumerate() {
                assert!(unit.loans().contains(end.loan()));
                assert_eq!(end.preceding_call_loan_ends(), rank);
                assert_eq!(end.preceding_drops(), 0);
                assert_eq!(
                    end.loan().call().source_unit(),
                    source.loan().call().source_unit()
                );
            }
            assert_eq!(source.preceding_call_loan_ends(), expected);
            assert_eq!(source.preceding_drops(), 0);
            paths.push(expected);
        }
        paths.sort();
        assert_eq!(paths, [1, 2]);
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().ends().is_empty());
    }
}

#[test]
fn early_operand_return_does_not_inherit_later_path_loan_ranks() {
    let observed = observe(
        "fun select(source: String, tail: Unit): borrow String from source = source\nfun sink(first: String, second: String, stop: Unit): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = select(owner, sink(if (flag) { return } else { \"first\" }, \"second\", return)) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let prefix = unit.borrow_call_prefix_ends();
        assert_eq!(prefix.len(), 2);
        assert_eq!(prefix[0].loan(), prefix[1].loan());
        assert_ne!(prefix[0].point(), prefix[1].point());
        let direct = unit.direct_call_loan_ends();
        let [second] = direct else {
            panic!("only the literal operand has direct publication proof")
        };
        assert_eq!(
            observed.sources.slice(second.loan().begin_span()),
            Ok("\"second\"")
        );
        assert_eq!(second.preceding_call_loan_ends(), 0);
        // Conditional operands are deliberately outside the direct slice. The
        // registered loan and source producer rank still preserve their actual token.
        assert_eq!(unit.loans().len(), 3);
        let conditional: Vec<_> = unit
            .loans()
            .iter()
            .filter(|loan| {
                observed.sources.slice(loan.begin_span())
                    == Ok("if (flag) { return } else { \"first\" }")
            })
            .collect();
        assert_eq!(conditional.len(), 1);
        assert!(!direct.iter().any(|end| end.loan() == conditional[0]));
        for end in direct {
            assert!(unit.loans().contains(end.loan()));
            assert_eq!(end.preceding_drops(), 0);
        }
        let mut ranks = Vec::new();
        for source in prefix {
            assert_eq!(source.preceding_drops(), 0);
            let expected = if source.point() == second.point() {
                2
            } else {
                0
            };
            assert_eq!(source.preceding_call_loan_ends(), expected);
            ranks.push(expected);
        }
        ranks.sort();
        assert_eq!(
            ranks,
            [0, 2],
            "early Return cannot acquire later-path tokens"
        );
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().ends().is_empty());
        assert!(unit.borrow_call_handoffs().is_empty());
    }
}
