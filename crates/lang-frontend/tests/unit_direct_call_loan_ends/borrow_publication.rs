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

#[test]
fn ownership_error_clears_previously_observed_call_ends() {
    let (observed, legacy) = observe_with_legacy(
        "fun read(text: String): Unit {}\nfun consume(own text: String): Unit {}\nfun run(): Unit { val text = \"held\"; read(text); consume(text); read(text) }",
    );
    let source = observed.parsed.source_id();
    let bytes = |span: Span| {
        assert_eq!(span.source_id(), source);
        (span.start(), span.end())
    };
    let [single] = legacy.diagnostics() else {
        panic!(
            "one legacy use-after-move diagnostic: {:?}",
            legacy.diagnostics()
        );
    };
    assert_eq!(single.code().to_string(), "L0131");
    assert_eq!(single.message(), "use of moved value");
    assert_eq!(bytes(single.primary_span()), (140, 144));
    let [lang_frontend::diagnostic::DiagnosticDetail::Label(moved)] = single.details() else {
        panic!("legacy retains only the move origin");
    };
    assert_eq!(moved.message(), "value was moved here");
    assert_eq!(bytes(moved.span()), (128, 132));
    assert!(legacy.loan_ends().is_empty());
    assert_fixed_ends(&observed, &[]);
    for unit in observed.units {
        let [diagnostic] = unit.diagnostics() else {
            panic!(
                "one Unit use-after-move diagnostic: {:?}",
                unit.diagnostics()
            );
        };
        assert_eq!(diagnostic.severity(), single.severity());
        assert_eq!(diagnostic.code(), single.code());
        assert_eq!(diagnostic.message(), single.message());
        assert_eq!(diagnostic.primary_span(), single.primary_span());
        let [
            origin,
            lang_frontend::diagnostic::DiagnosticDetail::Label(parameter),
        ] = diagnostic.details()
        else {
            panic!("move origin followed by the selected parameter");
        };
        assert_eq!(origin, &single.details()[0]);
        assert_eq!(parameter.message(), "selected parameter declared here");
        assert_eq!(bytes(parameter.span()), (9, 21));
        assert!(unit.deferred().is_empty());
        assert!(unit.loans().is_empty());
        assert!(unit.drops().is_empty());
        assert!(unit.validate().is_err());
    }
}

#[test]
fn later_real_deferred_work_blocks_the_event_capability() {
    let prefix_only =
        observe("fun read(text: String): Unit {}\nfun run(): Unit { read(\"before\") }");
    assert_units_admitted(&prefix_only);
    assert_fixed_ends(
        &prefix_only,
        &[ExpectedEnd {
            call: (50, 64),
            argument: (55, 63),
            parameter: (9, 21),
            point: ExpectedPoint::CallReturn,
            target: ExpectedTarget::Temporary,
        }],
    );

    let observed = observe(
        "fun read(text: String): Unit {}\nfun run(): Unit { read(\"before\"); var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
    );
    assert_fixed_ends(&observed, &[]);
    for (unit, names) in observed.units.into_iter().zip(&observed.unit_names) {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        let mut prefix_loans = unit
            .loans()
            .iter()
            .filter(|loan| observed.sources.slice(loan.end_span()) == Ok("read(\"before\")"));
        let prefix_loan = prefix_loans
            .next()
            .expect("the original prefix was visited");
        assert!(prefix_loans.next().is_none());
        assert_eq!(prefix_loan.kind(), LoanKind::Shared);
        assert_eq!(
            prefix_loan.target(),
            &UnitLoanTarget::Temporary(prefix_loan.argument())
        );
        assert_eq!(
            observed.sources.slice(prefix_loan.begin_span()),
            Ok("\"before\"")
        );

        let [deferred] = unit.deferred() else {
            panic!("one closure transport boundary: {:?}", unit.deferred());
        };
        let expected = *deferred;
        assert_eq!(
            deferred.reason(),
            OwnershipDeferredReason::OwnershipPrimitiveClosureTransport
        );
        let [source] = names.names().index().source_units() else {
            panic!("one original source");
        };
        let expression = deferred.expression();
        assert_eq!(expression.source_unit(), source.id());
        let span = observed
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .unwrap()
            .span();
        assert_eq!(span.source_id(), observed.parsed.source_id());
        assert_eq!((span.start(), span.end()), (109, 134));
        assert_eq!(
            observed.sources.slice(span),
            Ok("replace(&callback, { 2 })")
        );
        let rejected = unit
            .validate()
            .expect_err("deferred work cannot authorize codegen");
        assert_eq!(rejected.deferred(), &[expected]);
        assert!(rejected.direct_call_loan_ends().is_empty());
    }
}
