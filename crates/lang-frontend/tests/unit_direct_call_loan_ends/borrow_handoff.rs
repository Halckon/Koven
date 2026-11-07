//! A normal-return handoff is not a complete loan-end stream.
use super::*;

fn assert_handoff_identity(observed: &Observed) {
    for (entry, (unit, names)) in observed.units.iter().zip(&observed.unit_names).enumerate() {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source")
        };
        assert_eq!(indexed.source_id(), observed.parsed.source_id());
        match (entry, indexed.key()) {
            (0, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "call_ends.ko");
            }
            (1, SourceUnitKey::MemoryDocument) => {}
            _ => panic!("requested source entry"),
        }
        for fact in unit.borrow_call_handoffs() {
            assert_eq!(fact.source_loan().call().source_unit(), indexed.id());
            assert_eq!(fact.source_loan().argument().source_unit(), indexed.id());
            assert_eq!(fact.binding().source_unit(), indexed.id());
            let UnitLoanTarget::Place(place) = fact.source_loan().target() else {
                panic!("root place")
            };
            assert_eq!(place.root().source_unit(), indexed.id());
        }
    }
}

#[test]
fn named_borrow_return_hands_off_only_its_actual_source_token() {
    let observed = observe(
        "fun select(other: String, source: String): borrow String from source = source\nfun run(): Unit { val source = \"kept\"; val other = \"other\"; borrow val item = select(source = source, other = other); println(item) }",
    );
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one actual return handoff")
        };
        let call_node = observed
            .parsed
            .ast()
            .expressions()
            .get(handoff.source_loan().call().expression())
            .unwrap();
        let argument_node = observed
            .parsed
            .ast()
            .expressions()
            .get(handoff.source_loan().argument().expression())
            .unwrap();
        assert_eq!(
            observed.sources.slice(call_node.span()),
            Ok("select(source = source, other = other)")
        );
        assert_eq!(observed.sources.slice(argument_node.span()), Ok("source"));
        assert_eq!(
            observed
                .sources
                .slice(handoff.source_loan().parameter_span().unwrap()),
            Ok("source: String")
        );
        assert_eq!(call_node.span().source_id(), observed.parsed.source_id());
        let binding = &unit.borrow_results().bindings()[0];
        let source = binding.source_loan().unwrap();
        assert_eq!(handoff.binding(), binding.binding());
        assert_eq!(handoff.source_loan().call(), source.call());
        assert_eq!(handoff.source_loan().argument(), source.argument());
        assert_eq!(handoff.source_loan().target(), binding.origin());
        assert_eq!(handoff.point(), UnitDropPoint::CallReturn(source.call()));
        assert!(unit.loans().contains(handoff.source_loan()));
    }
}

const VIEW: &str = "fun view(source: String): borrow String from source = source\n";

#[test]
fn direct_binding_keeps_its_owner_until_the_result_endpoint() {
    let observed = observe_with_legacy(&format!(
        "{VIEW}fun run(): Unit {{ val source = \"kept\"; borrow val item = view(source); println(item) }}"
    ));
    let (observed, single) = observed;
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    let old_source = single.borrow_results().bindings()[0].source_loan().unwrap();
    assert!(
        !single
            .loan_ends()
            .iter()
            .any(|end| end.call() == old_source.call() && end.argument() == old_source.argument())
    );
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one handoff")
        };
        assert_eq!(handoff.source_loan().call().expression(), old_source.call());
        assert_eq!(
            handoff.source_loan().argument().expression(),
            old_source.argument()
        );
        let UnitLoanTarget::Place(origin) = handoff.source_loan().target() else {
            panic!("root source")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(origin.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_ne!(drops[0].point(), handoff.point());
        assert_eq!(drops[0].point(), unit.borrow_results().ends()[0].point());
    }
}

#[test]
fn same_root_arguments_still_select_the_declared_source_identity() {
    let observed = observe(
        "fun select(other: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(source = owner, other = owner); println(item) }",
    );
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one handoff")
        };
        let source = unit.borrow_results().bindings()[0].source_loan().unwrap();
        assert_eq!(handoff.source_loan().argument(), source.argument());
        let same_call: Vec<_> = unit
            .loans()
            .iter()
            .filter(|loan| loan.call() == source.call())
            .collect();
        assert_eq!(same_call.len(), 2);
        assert_eq!(same_call[0].target(), same_call[1].target());
        assert_ne!(same_call[0].argument(), same_call[1].argument());
    }
}

#[test]
fn abandoned_initializer_does_not_commit_a_handoff() {
    let observed = observe(
        "fun select(source: String, other: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, return); println(item) }",
    );
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    for unit in &observed.units {
        // The second operand abandons the call before binding initialization.
        assert!(unit.borrow_call_handoffs().is_empty());
    }
}

#[test]
fn unsupported_consumers_and_parent_sources_do_not_gain_handoffs() {
    for body in [
        "fun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = view(parent); println(child) }",
        "fun observe(text: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; observe(view(owner)) }",
        "fun run(): Unit { val owner = \"kept\"; borrow val item = (view(owner)); println(item) }",
    ] {
        let observed = observe(&format!("{VIEW}{body}"));
        assert_units_admitted(&observed);
        assert_handoff_identity(&observed);
        for unit in &observed.units {
            if body.contains("borrow val parent") {
                let [fact] = unit.borrow_call_handoffs() else {
                    panic!("only parent handoff")
                };
                let call = observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(fact.source_loan().call().expression())
                    .unwrap();
                assert_eq!(observed.sources.slice(call.span()), Ok("view(owner)"));
                assert_eq!(
                    fact.binding(),
                    unit.borrow_results().bindings()[0].binding()
                );
            }
            assert_eq!(
                unit.borrow_call_handoffs().len(),
                usize::from(body.contains("borrow val parent"))
            );
        }
    }
}

#[test]
fn later_errors_and_deferred_work_revoke_handoffs() {
    let good = format!(
        "{VIEW}fun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); println(item)"
    );
    let baseline = observe(&format!("{good} }}"));
    assert_units_admitted(&baseline);
    assert_handoff_identity(&baseline);
    for unit in &baseline.units {
        assert_eq!(unit.borrow_call_handoffs().len(), 1);
    }
    for (suffix, error) in [
        ("; val moved = owner; println(owner)", true),
        (
            "; var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 })",
            false,
        ),
    ] {
        let observed = observe(&format!("{good}{suffix} }}"));
        for unit in &observed.units {
            if error {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|diagnostic| diagnostic.code().to_string() == "L0131"),
                    "{:?}",
                    unit.diagnostics()
                );
            } else {
                assert!(unit.diagnostics().is_empty());
                assert!(!unit.deferred().is_empty());
            }
            assert!(unit.borrow_call_handoffs().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}
