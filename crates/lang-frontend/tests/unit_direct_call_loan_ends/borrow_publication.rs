//! Failed whole-unit publication cannot retain a partial borrow-event stream.
use super::*;

#[test]
fn loop_borrow_initializers_keep_the_existing_rejection_boundary() {
    for jump in ["break", "continue"] {
        let text = format!(
            "fun select(source: String, tail: Unit): borrow String from source = source\nfun run(): Unit {{ loop {{ val owner = \"kept\"; borrow val item = select(owner, {jump}) }} }}"
        );
        let observed = observe(&text);
        for unit in &observed.units {
            let [diagnostic] = unit.diagnostics() else {
                panic!("one existing loop-boundary error");
            };
            assert_eq!(diagnostic.code().to_string(), "L0164");
            assert_eq!(
                observed.sources.slice(diagnostic.primary_span()),
                Ok("borrow")
            );
            assert!(unit.deferred().is_empty());
            assert!(unit.borrow_results().bindings().is_empty());
            assert!(unit.borrow_call_handoffs().is_empty());
            assert!(unit.borrow_call_prefix_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn both_entries_revoke_all_three_populated_borrow_event_tables() {
    let prefix = "fun view(source: String, aux: String): borrow String from source = source\nfun prefix(source: String, tail: Unit): borrow String from source = source\nfun sink(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; val aux = \"aux\"; sink(aux); borrow val item = view(owner, aux); println(item) }\nfun abandon(): Unit { val root = \"root\"; borrow val item = prefix(root, return) }";
    let accepted = observe(prefix);
    assert_units_admitted(&accepted);
    for unit in &accepted.units {
        assert_eq!(unit.direct_call_loan_ends().len(), 1);
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one actual handoff");
        };
        assert_eq!(handoff.auxiliary_loan_ends().len(), 1);
        assert_eq!(unit.borrow_call_prefix_ends().len(), 1);
        assert!(unit.loans().contains(handoff.source_loan()));
        assert!(unit.loans().contains(&handoff.auxiliary_loan_ends()[0]));
    }
    for (suffix, deferred) in [
        ("fun rejected(): Unit { val target = 0; target = 1 }", false),
        (
            "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
            true,
        ),
    ] {
        let text = format!("{prefix}\n{suffix}");
        let rejected = observe(&text);
        for unit in &rejected.units {
            if deferred {
                assert!(unit.diagnostics().is_empty());
                let [fact] = unit.deferred() else {
                    panic!("one actual deferred operation");
                };
                assert_eq!(
                    fact.reason(),
                    OwnershipDeferredReason::OwnershipPrimitiveClosureTransport
                );
                let node = rejected
                    .parsed
                    .ast()
                    .expressions()
                    .get(fact.expression().expression())
                    .unwrap();
                assert_eq!(
                    rejected.sources.slice(node.span()),
                    Ok("replace(&callback, { 2 })")
                );
            } else {
                assert!(unit.deferred().is_empty());
                let [diagnostic] = unit.diagnostics() else {
                    panic!("one actual immutable inout error");
                };
                assert_eq!(diagnostic.code().to_string(), "L0134");
                let span = diagnostic.primary_span();
                assert_eq!(span.source_id(), rejected.parsed.source_id());
                assert_eq!(
                    (span.start(), span.end()),
                    (
                        text.rfind("target").unwrap(),
                        text.rfind("target").unwrap() + 6
                    )
                );
                assert_eq!(rejected.sources.slice(span), Ok("target"));
            }
            assert!(unit.direct_call_loan_ends().is_empty());
            assert!(unit.borrow_call_handoffs().is_empty());
            assert!(unit.borrow_call_prefix_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}
