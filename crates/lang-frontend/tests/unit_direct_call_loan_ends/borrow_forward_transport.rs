//! Forwarding moves a real source token to the actual function return; it does not end it.
use super::*;

const VIEW: &str = "fun view(source: String): borrow String from source = source\n";

#[test]
fn actual_source_token_crosses_inner_and_wrapper_returns_without_ending() {
    for body in [
        "= view(source)",
        "{ return view(source) }",
        "{ return ((view((source)))) }",
    ] {
        let observed = observe(&format!(
            "{VIEW}fun wrap(source: String): borrow String from source {body}"
        ));
        assert_units_admitted(&observed);
        for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
            let [transport] = unit.borrow_forward_transports() else {
                panic!("one actual forwarded source token")
            };
            let [indexed] = names.names().index().source_units() else {
                panic!("one source")
            };
            let text = |id: UnitExpressionId| {
                assert_eq!(id.source_unit(), indexed.id());
                let span = observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(id.expression())
                    .unwrap()
                    .span();
                assert_eq!(span.source_id(), observed.parsed.source_id());
                observed.sources.slice(span).unwrap()
            };
            let source = transport.source_loan();
            assert_eq!(source.kind(), LoanKind::Shared);
            assert!(unit.loans().contains(source));
            assert_eq!(
                text(source.call()),
                if body.contains("((") {
                    "view((source))"
                } else {
                    "view(source)"
                }
            );
            assert_eq!(
                text(source.argument()),
                if body.contains("((") {
                    "(source)"
                } else {
                    "source"
                }
            );
            assert_eq!(
                transport.handoff_point(),
                UnitDropPoint::CallReturn(source.call())
            );
            let UnitDropPoint::ControlTransfer(returned) = transport.point() else {
                panic!("actual wrapper return")
            };
            assert_eq!(
                text(returned),
                match body {
                    "= view(source)" => "view(source)",
                    "{ return view(source) }" => "return view(source)",
                    _ => "return ((view((source))))",
                }
            );
            assert_eq!(
                text(transport.returned_expression()),
                if body.contains("((") {
                    "((view((source))))"
                } else {
                    "view(source)"
                }
            );
            let [forwarded] = unit.borrow_results().forwarded_source_loans() else {
                panic!("one verified source")
            };
            assert_eq!(source.call(), forwarded.call());
            assert_eq!(source.argument(), forwarded.argument());
            let UnitLoanTarget::Place(place) = source.target() else {
                panic!("shared parameter root")
            };
            assert!(place.is_root());
            assert!(!unit.formed_owned_locals().contains(&place.root()));
            assert!(
                !unit
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            );
            assert!(unit.borrow_binding_transports().is_empty());
            assert!(unit.borrow_call_transports().is_empty());
        }
    }
}

#[test]
fn consumer_return_suffix_still_requires_a_proven_continuation() {
    let observed = observe(&format!(
        "{VIEW}fun observe(item: String, own n: Int): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; observe(view(owner), return) }}"
    ));
    for unit in &observed.units {
        let [diagnostic] = unit.diagnostics() else {
            panic!("existing unsupported consumer path")
        };
        assert_eq!(diagnostic.code().to_string(), "L0164");
        assert_eq!(
            observed.sources.slice(diagnostic.primary_span()),
            Ok("view(owner)")
        );
        assert!(unit.borrow_call_transports().is_empty());
        assert!(unit.borrow_forward_transports().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}

#[test]
fn named_same_root_operands_forward_only_the_declared_source_token() {
    let observed = observe(
        "fun select(other: String, source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source { return select(source = source, other = source) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [transport] = unit.borrow_forward_transports() else {
            panic!("one source, not its same-root auxiliary")
        };
        let source = transport.source_loan();
        assert_eq!(
            observed.sources.slice(source.parameter_span().unwrap()),
            Ok("source: String")
        );
        let loans: Vec<_> = unit
            .loans()
            .iter()
            .filter(|loan| loan.call() == source.call())
            .collect();
        assert_eq!(loans.len(), 2);
        assert_eq!(loans[0].target(), loans[1].target());
        assert_ne!(loans[0].argument(), loans[1].argument());
        let argument = observed
            .parsed
            .ast()
            .expressions()
            .get(source.argument().expression())
            .unwrap()
            .span();
        // Original source operand precedes the auxiliary in the named source order.
        let auxiliary = loans
            .iter()
            .find(|loan| loan.argument() != source.argument())
            .unwrap();
        let auxiliary_span = observed
            .parsed
            .ast()
            .expressions()
            .get(auxiliary.argument().expression())
            .unwrap()
            .span();
        assert!(argument.start() < auxiliary_span.start());
        assert_eq!(
            observed.sources.slice(auxiliary.parameter_span().unwrap()),
            Ok("other: String")
        );
        assert!(
            unit.drops().is_empty(),
            "shared parameters never gain payload ownership"
        );
    }
}

#[test]
fn wrapper_chain_keeps_each_function_source_identity_separate_from_the_caller() {
    let observed = observe(&format!(
        "{VIEW}fun wrap(source: String): borrow String from source = view(source)\nfun wrapAgain(source: String): borrow String from source {{ return wrap(source) }}\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = wrapAgain(owner); println(item) }}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [first, second] = unit.borrow_forward_transports() else {
            panic!("one actual transfer per wrapper")
        };
        assert_ne!(first.source_loan().call(), second.source_loan().call());
        assert_ne!(first.source_loan().target(), second.source_loan().target());
        let [caller] = unit.borrow_binding_transports() else {
            panic!("independent caller transaction")
        };
        assert_ne!(caller.source_loan().target(), first.source_loan().target());
        assert_ne!(caller.source_loan().target(), second.source_loan().target());
        for forwarded in [first, second] {
            let UnitLoanTarget::Place(place) = forwarded.source_loan().target() else {
                panic!("shared parameter root")
            };
            assert!(
                !unit
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            );
        }
    }
}

#[test]
fn uncompleted_or_unreachable_forwarded_calls_do_not_transfer_tokens() {
    for body in [
        "{ return select(source, return source) }",
        "{ return source; return select(source, 0) }",
    ] {
        let observed = observe(&format!(
            "fun select(source: String, own n: Int): borrow String from source = source\nfun wrap(source: String): borrow String from source {body}"
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(unit.borrow_forward_transports().is_empty());
        }
    }
}

#[test]
fn projected_source_remains_outside_the_forward_transport_slice() {
    let observed = observe(&format!(
        "{VIEW}class Holder(val text: String) {{}}\nfun wrap(source: Holder): borrow String from source = view(source.text)"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 1);
        assert!(unit.borrow_forward_transports().is_empty());
    }
}

#[test]
fn later_error_and_deferred_revoke_a_completed_forward_transport_prefix() {
    let prefix =
        format!("{VIEW}fun wrap(source: String): borrow String from source = view(source)");
    let good = observe(&prefix);
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.borrow_forward_transports().len(), 1);
    }
    for suffix in [
        "fun rejected(): Unit { val owner = \"kept\"; val moved = owner; println(owner) }",
        "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
    ] {
        let observed = observe(&format!("{prefix}\n{suffix}"));
        for unit in &observed.units {
            assert!(!unit.diagnostics().is_empty() || !unit.deferred().is_empty());
            assert!(unit.borrow_forward_transports().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn branch_returns_preserve_distinct_actual_handoffs_for_the_same_parameter() {
    let observed = observe(&format!(
        "{VIEW}fun wrap(source: String, flag: Boolean): borrow String from source {{ if (flag) {{ return view(source) }}; return ((view(source))) }}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [first, second] = unit.borrow_forward_transports() else {
            panic!("two mutually exclusive executed return sites")
        };
        assert_eq!(first.source_loan().target(), second.source_loan().target());
        assert_ne!(first.source_loan().call(), second.source_loan().call());
        assert_ne!(
            first.source_loan().argument(),
            second.source_loan().argument()
        );
        assert_ne!(first.point(), second.point());
        for (transport, expected) in [
            (first, "return view(source)"),
            (second, "return ((view(source)))"),
        ] {
            let UnitDropPoint::ControlTransfer(returned) = transport.point() else {
                panic!("actual branch return")
            };
            let span = observed
                .parsed
                .ast()
                .expressions()
                .get(returned.expression())
                .unwrap()
                .span();
            assert_eq!(observed.sources.slice(span), Ok(expected));
        }
        assert!(
            unit.drops().is_empty(),
            "neither return owns the borrowed parameter"
        );
    }
}

#[test]
fn forwarding_does_not_delay_unrelated_owned_auxiliary_cleanup() {
    let observed = observe(
        "fun select(other: String, source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source { val auxiliary = \"released\"; return select(other = auxiliary, source = source) }",
    );
    assert_units_admitted(&observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [transport] = unit.borrow_forward_transports() else {
            panic!("one borrowed source")
        };
        let [drop] = unit.drops() else {
            panic!("only the owned auxiliary is dropped")
        };
        let UnitDropTarget::Named(auxiliary) = drop.target() else {
            panic!("owned local")
        };
        let [local] = names.names().source_units() else {
            panic!("one source")
        };
        let span = local.resolution().symbols()[auxiliary.symbol().index()].span();
        assert_eq!(observed.sources.slice(span), Ok("auxiliary"));
        assert_eq!(drop.point(), transport.handoff_point());
        assert_ne!(drop.point(), transport.point());
        let UnitLoanTarget::Place(source) = transport.source_loan().target() else {
            panic!("shared source")
        };
        assert_ne!(source.root(), auxiliary);
    }
}
