//! Actual source tokens cross inner return before their consumer can retire them.
use super::*;

#[test]
fn nested_source_token_reaches_actual_consumer_before_ending() {
    for operand in ["view(owner)", "((view((owner))))"] {
        let observed = observe(&format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; observe({operand}) }}"
        ));
        assert_units_admitted(&observed);
        for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
            let [transport] = unit.borrow_call_transports() else {
                panic!("one actual source-to-consumer transport")
            };
            let [indexed] = names.names().index().source_units() else {
                panic!("one source")
            };
            let text = |expression: UnitExpressionId| {
                assert_eq!(expression.source_unit(), indexed.id());
                observed
                    .sources
                    .slice(
                        observed
                            .parsed
                            .ast()
                            .expressions()
                            .get(expression.expression())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
            };
            let [consumer, source] = transport.ended_loans();
            assert_eq!(
                text(source.call()),
                if operand.starts_with("((") {
                    "view((owner))"
                } else {
                    "view(owner)"
                }
            );
            assert_eq!(text(consumer.call()), format!("observe({operand})"));
            assert_eq!(text(consumer.argument()), operand);
            assert_eq!(consumer.target(), source.target());
            assert_eq!(consumer.kind(), LoanKind::Shared);
            assert_eq!(source.kind(), LoanKind::Shared);
            assert!(unit.loans().contains(consumer));
            assert!(unit.loans().contains(source));
            assert_ne!(source.call(), consumer.call());
            assert_eq!(
                transport.point(),
                UnitDropPoint::CallReturn(consumer.call())
            );
            assert_eq!(
                transport.handoff_point(),
                UnitDropPoint::CallReturn(source.call())
            );
            let [call_use] = unit.borrow_results().call_uses() else {
                panic!("one verified continuation")
            };
            assert_eq!(call_use.source_loan().call(), source.call());
            assert_eq!(call_use.source_loan().argument(), source.argument());
            assert_eq!(call_use.call(), consumer.call());
            assert_eq!(call_use.argument(), consumer.argument());
            let UnitLoanTarget::Place(place) = source.target() else {
                panic!("root")
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), transport.point());
        }
    }
}

#[test]
fn uncompleted_consumer_and_later_error_do_not_publish_transport() {
    for tail in [
        "observe(view(owner), return)",
        "observe(view(owner), 0); val moved = owner; println(owner)",
    ] {
        let observed = observe(&format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String, own n: Int): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; {tail} }}"
        ));
        for unit in &observed.units {
            assert!(unit.borrow_call_transports().is_empty());
        }
    }
}

#[test]
fn named_same_root_consumers_retain_distinct_source_tokens() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun observe(first: String, second: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; observe(second = view(owner), first = view(owner)) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let transports = unit.borrow_call_transports();
        assert_eq!(transports.len(), 2);
        let bytes = |id: UnitExpressionId| {
            let span = observed
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span();
            assert_eq!(span.source_id(), observed.parsed.source_id());
            (span.start(), span.end())
        };
        let actual: Vec<_> = transports
            .iter()
            .map(|transport| {
                let [consumer, source] = transport.ended_loans();
                assert_eq!(source.target(), consumer.target());
                assert_eq!(
                    transport.point(),
                    UnitDropPoint::CallReturn(consumer.call())
                );
                (
                    bytes(consumer.call()),
                    bytes(consumer.argument()),
                    bytes(source.call()),
                    bytes(source.argument()),
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                ((151, 201), (189, 200), (189, 200), (194, 199)),
                ((151, 201), (168, 179), (168, 179), (173, 178)),
            ]
        );
        assert_ne!(
            transports[0].ended_loans()[1].call(),
            transports[1].ended_loans()[1].call()
        );
    }
}

#[test]
fn deferred_work_revokes_a_nonempty_transport_prefix() {
    let prefix = "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; observe(view(owner))";
    let good = observe(&format!("{prefix} }}"));
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.borrow_call_transports().len(), 1);
    }
    let deferred = observe(&format!(
        "{prefix}; var callback: () -> Int = {{ 1 }}; val old = replace(&callback, {{ 2 }}) }}"
    ));
    for unit in &deferred.units {
        assert!(unit.diagnostics().is_empty());
        assert!(!unit.deferred().is_empty());
        assert!(unit.borrow_call_transports().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}

#[test]
fn later_cfg_remains_closed_without_publishing_transport() {
    for tail in ["if (flag) 1 else 2", "if (flag) return else 0"] {
        let observed = observe(&format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String, own n: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ val owner = \"kept\"; observe(view(owner), {tail}) }}"
        ));
        for unit in &observed.units {
            let [diagnostic] = unit.diagnostics() else {
                panic!("existing unproved continuation")
            };
            assert_eq!(diagnostic.code().to_string(), "L0164");
            assert_eq!(
                diagnostic.primary_span().source_id(),
                observed.parsed.source_id()
            );
            assert_eq!(
                (
                    diagnostic.primary_span().start(),
                    diagnostic.primary_span().end()
                ),
                (167, 178)
            );
            assert_eq!(
                observed.sources.slice(diagnostic.primary_span()),
                Ok("view(owner)")
            );
            assert!(unit.borrow_call_transports().is_empty());
            assert!(unit.borrow_results().call_uses().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}
