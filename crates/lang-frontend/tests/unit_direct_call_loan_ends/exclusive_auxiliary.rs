//! Unrelated exclusive call loans retire without ending the Shared result source.
use super::*;

#[test]
fn exclusive_auxiliary_baseline_publishes_the_actual_token() {
    let observed = observe(
        r#"fun view(source: String, inout aux: String): borrow String from source = source
fun run(): Unit { val owner = "kept"; var other = "other"; borrow val item = view(owner, &other); println(item); println(other) }"#,
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert_eq!(unit.borrow_results().bindings().len(), 1);
        assert_eq!(
            unit.loans()
                .iter()
                .filter(|loan| loan.kind() == LoanKind::Exclusive)
                .count(),
            1
        );
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one completed handoff");
        };
        let [auxiliary] = handoff.auxiliary_loan_ends() else {
            panic!("one actual exclusive retirement");
        };
        assert_eq!(auxiliary.kind(), LoanKind::Exclusive);
        assert!(unit.loans().contains(auxiliary));
        assert_eq!(handoff.source_loan().kind(), LoanKind::Shared);
        assert_ne!(auxiliary.target(), handoff.source_loan().target());
    }
}

fn assert_handoff(observed: &Observed, call_text: &str, expected: &[(&str, &str, LoanKind)]) {
    assert_handoff_with_prefixes(observed, call_text, expected, 0, "println(item)");
}

fn assert_handoff_with_prefixes(
    observed: &Observed,
    call_text: &str,
    expected: &[(&str, &str, LoanKind)],
    prefixes: usize,
    endpoint_text: &str,
) {
    assert_units_admitted(observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [indexed] = names.names().index().source_units() else {
            panic!("one actual source");
        };
        let [local] = names.names().source_units() else {
            panic!("one local resolution");
        };
        let node = |expression: UnitExpressionId| {
            assert_eq!(expression.source_unit(), indexed.id());
            observed
                .parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .unwrap()
        };
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one completed direct initializer");
        };
        let source = handoff.source_loan();
        assert_eq!(source.kind(), LoanKind::Shared);
        assert!(unit.loans().contains(source));
        assert_eq!(
            observed.sources.slice(node(source.call()).span()),
            Ok(call_text)
        );
        assert_eq!(
            observed.sources.slice(node(source.argument()).span()),
            Ok("owner")
        );
        assert_eq!(
            observed.sources.slice(source.parameter_span().unwrap()),
            Ok("source: String")
        );
        let UnitLoanTarget::Place(source_place) = source.target() else {
            panic!("Shared owned root source");
        };
        assert!(source_place.is_root());
        assert!(unit.formed_owned_locals().contains(&source_place.root()));
        assert_eq!(handoff.point(), UnitDropPoint::CallReturn(source.call()));
        let binding = unit
            .borrow_results()
            .bindings()
            .iter()
            .find(|binding| binding.binding() == handoff.binding())
            .unwrap();
        assert_eq!(binding.initializer(), source.call());
        assert_eq!(binding.origin(), source.target());
        assert_eq!(binding.source_loan().unwrap().argument(), source.argument());
        assert_eq!(handoff.auxiliary_loan_ends().len(), expected.len());
        for (loan, &(argument_text, parameter_text, kind)) in
            handoff.auxiliary_loan_ends().iter().zip(expected)
        {
            assert!(unit.loans().contains(loan));
            assert_eq!(loan.kind(), kind);
            assert_eq!(loan.call(), source.call());
            assert_ne!(loan.argument(), source.argument());
            assert_eq!(loan.end_span(), node(source.call()).span());
            assert_eq!(
                observed.sources.slice(node(loan.argument()).span()),
                Ok(argument_text)
            );
            let parameter = loan.parameter_span().unwrap();
            assert_eq!(parameter.source_id(), observed.parsed.source_id());
            assert_eq!(observed.sources.slice(parameter), Ok(parameter_text));
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("owned auxiliary root");
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), indexed.id());
            assert!(unit.formed_owned_locals().contains(&place.root()));
            let symbol = &local.resolution().symbols()[place.root().symbol().index()];
            let root_name = argument_text.trim_matches(['(', ')']);
            assert_eq!(symbol.name(), root_name);
            assert_eq!(observed.sources.slice(symbol.span()), Ok(root_name));
            if kind == LoanKind::Exclusive {
                assert_ne!(place, source_place);
            }
            let Expression::Call { arguments, .. } = node(loan.call()).payload() else {
                panic!("actual call");
            };
            let argument = arguments
                .iter()
                .find(|argument| argument.value == loan.argument().expression())
                .unwrap();
            match kind {
                LoanKind::Exclusive => {
                    let Some(lang_frontend::parser::ParameterModeMarker::Inout(marker)) =
                        argument.mode_marker
                    else {
                        panic!("actual exclusive mode marker");
                    };
                    assert_eq!(loan.begin_span(), marker);
                    assert_eq!(observed.sources.slice(marker), Ok("&"));
                    assert_ne!(marker, node(loan.argument()).span());
                }
                LoanKind::Shared => {
                    assert!(argument.mode_marker.is_none());
                    assert_eq!(loan.begin_span(), node(loan.argument()).span());
                }
            }
        }
        assert!(
            unit.direct_call_loan_ends()
                .iter()
                .all(|event| event.loan().call() != source.call())
        );
        assert!(
            unit.exclusive_call_prefix_ends()
                .iter()
                .all(|event| event.loan().call() != source.call())
        );
        let source_drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(source_place.root()))
            .collect();
        assert_eq!(source_drops.len(), 1 + prefixes);
        assert_eq!(unit.borrow_call_prefix_ends().len(), prefixes);
        let [end] = unit.borrow_results().ends() else {
            panic!("one result end");
        };
        assert_eq!(end.binding(), handoff.binding());
        let UnitDropPoint::AfterStatement(last_use) = end.point() else {
            panic!("normal result retires at an actual statement boundary");
        };
        assert_eq!(last_use.source_unit(), indexed.id());
        let last_use = observed
            .parsed
            .ast()
            .statements()
            .get(last_use.statement())
            .unwrap();
        assert_eq!(observed.sources.slice(last_use.span()), Ok(endpoint_text));
        assert_eq!(
            source_drops
                .iter()
                .filter(|drop| drop.point() == end.point())
                .count(),
            1
        );
        assert!(
            source_drops
                .iter()
                .all(|drop| drop.point() != handoff.point())
        );
        let [transport] = unit.borrow_binding_transports() else {
            panic!("source token still transported");
        };
        assert_eq!(transport.source_loan(), source);
        assert_eq!(transport.binding(), handoff.binding());
        assert_eq!(transport.point(), end.point());
    }
}

#[test]
fn named_mixed_auxiliaries_keep_pending_order_and_original_markers() {
    let call = "view(last = &((last)), source = owner, shared = first, first = &(first))";
    let observed = observe(&format!(
        r#"fun view(inout first: String, source: String, shared: String, inout last: String): borrow String from source = source
fun run(): Unit {{ val owner = "kept"; var first = "first"; var last = "last"; borrow val item = {call}; println(item) }}"#
    ));
    // The shared and exclusive loans overlap, so this is an intentionally invalid order.
    for unit in &observed.units {
        assert!(
            unit.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135")
        );
        assert!(unit.borrow_call_handoffs().is_empty());
    }
    let call = "view(last = &((last)), source = owner, shared = shared, first = &(first))";
    let observed = observe(&format!(
        r#"fun view(inout first: String, source: String, shared: String, inout last: String): borrow String from source = source
fun run(): Unit {{ val owner = "kept"; var first = "first"; var last = "last"; val shared = "shared"; borrow val item = {call}; println(item) }}"#
    ));
    assert_handoff(
        &observed,
        call,
        &[
            ("((last))", "inout last: String", LoanKind::Exclusive),
            ("shared", "shared: String", LoanKind::Shared),
            ("(first)", "inout first: String", LoanKind::Exclusive),
        ],
    );
}

#[test]
fn completed_auxiliary_can_change_while_source_stays_borrowed() {
    let prefix = r#"fun view(source: String, inout aux: String): borrow String from source = source
fun run(): Unit { var owner = "kept"; var other = "other"; borrow val item = view(owner, &other);"#;
    let observed = observe(&format!(
        r#"{prefix} val old = replace(&other, "new"); println(item); println(old) }}"#
    ));
    assert_handoff(
        &observed,
        "view(owner, &other)",
        &[("other", "inout aux: String", LoanKind::Exclusive)],
    );
    let blocked = observe(&format!(
        r#"{prefix} val old = replace(&owner, "new"); println(item); println(old) }}"#
    ));
    for unit in &blocked.units {
        assert!(
            unit.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
            "{:?}",
            unit.diagnostics()
        );
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.borrow_binding_transports().is_empty());
    }
    let overlap = observe(
        r#"fun view(source: String, inout aux: String): borrow String from source = source
fun run(): Unit { var owner = "kept"; borrow val item = view(owner, &owner); println(item) }"#,
    );
    for unit in &overlap.units {
        assert!(
            unit.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135")
        );
        assert!(unit.borrow_call_handoffs().is_empty());
    }
}

#[test]
fn return_prefix_does_not_claim_exclusive_normal_completion() {
    for (tail, expected_handoffs) in [("return", 0), ("if (flag) { return } else { 0 }", 1)] {
        let call = format!("view(owner, &other, {tail})");
        let observed = observe(&format!(
            r#"fun view(source: String, inout aux: String, own next: Int): borrow String from source = source
fun run(flag: Boolean): Unit {{ val owner = "kept"; var other = "other"; borrow val item = {call}; println(item) }}"#
        ));
        assert_units_admitted(&observed);
        if expected_handoffs == 1 {
            assert_handoff_with_prefixes(
                &observed,
                &call,
                &[("other", "inout aux: String", LoanKind::Exclusive)],
                1,
                &format!(
                    r#"{{ val owner = "kept"; var other = "other"; borrow val item = {call}; println(item) }}"#
                ),
            );
        }
        for unit in &observed.units {
            assert_eq!(unit.borrow_call_handoffs().len(), expected_handoffs);
            let [prefix] = unit.borrow_call_prefix_ends() else {
                panic!("one real abandoned source");
            };
            assert_eq!(prefix.loan().kind(), LoanKind::Shared);
            assert_eq!(
                prefix.preceding_call_loan_ends(),
                1,
                "actual exclusive end precedes source on unwind"
            );
            assert!(
                unit.exclusive_call_prefix_ends().is_empty(),
                "Owned-result prefix slice is unchanged"
            );
            let UnitDropPoint::ControlTransfer(returned) = prefix.point() else {
                panic!("actual return");
            };
            assert_eq!(
                observed.sources.slice(
                    observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(returned.expression())
                        .unwrap()
                        .span()
                ),
                Ok("return")
            );
            let exclusive = unit
                .loans()
                .iter()
                .find(|loan| {
                    loan.call() == prefix.loan().call() && loan.kind() == LoanKind::Exclusive
                })
                .unwrap();
            let UnitLoanTarget::Place(place) = exclusive.target() else {
                panic!("exclusive owned root");
            };
            assert_eq!(
                unit.drops()
                    .iter()
                    .filter(|drop| drop.point() == prefix.point()
                        && drop.target() == UnitDropTarget::Named(place.root()))
                    .count(),
                1
            );
        }
    }
    let before = observe(
        r#"fun view(own next: Int, source: String, inout aux: String): borrow String from source = source
fun run(): Unit { val owner = "kept"; var other = "other"; borrow val item = view(return, owner, &other); println(item) }"#,
    );
    assert_units_admitted(&before);
    for unit in &before.units {
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.borrow_call_prefix_ends().is_empty());
        assert!(
            unit.loans()
                .iter()
                .all(|loan| loan.kind() != LoanKind::Exclusive)
        );
    }
    let observed = observe(
        r#"fun view(source: String, inout aux: String): borrow String from source = source
fun run(): Unit { val owner = "kept"; var other = "other"; return; borrow val item = view(owner, &other); println(item) }"#,
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.borrow_call_prefix_ends().is_empty());
    }
}

#[test]
fn non_owned_and_projected_exclusive_operands_keep_their_boundaries() {
    let observed = observe(
        r#"fun view(source: String, inout aux: String): borrow String from source = source
fun run(inout other: String): Unit { val owner = "kept"; borrow val item = view(owner, &other); println(item) }"#,
    );
    assert_handoff(&observed, "view(owner, &other)", &[]);
    for unit in &observed.units {
        assert_eq!(
            unit.loans()
                .iter()
                .filter(|loan| loan.kind() == LoanKind::Exclusive)
                .count(),
            1
        );
    }
    for (declaration, prefix, argument) in [
        ("inout aux: Int", "var other = 0;", "other"),
        (
            "inout aux: List<String>",
            r#"var other = listOf("element"); for (entry in other) { println(entry) };"#,
            "other",
        ),
    ] {
        let observed = observe(&format!(
            r#"fun view(source: String, {declaration}): borrow String from source = source
fun run(): Unit {{ val owner = "kept"; {prefix} borrow val item = view(owner, &{argument}); println(item) }}"#
        ));
        let endpoint = if declaration == "inout aux: Int" {
            "println(item)".to_owned()
        } else {
            format!(
                r#"{{ val owner = "kept"; {prefix} borrow val item = view(owner, &other); println(item) }}"#
            )
        };
        assert_handoff_with_prefixes(&observed, "view(owner, &other)", &[], 0, &endpoint);
        for unit in &observed.units {
            assert_eq!(
                unit.loans()
                    .iter()
                    .filter(|loan| loan.kind() == LoanKind::Exclusive)
                    .count(),
                1
            );
        }
    }
    let observed = observe(
        r#"class Holder(var text: String)
fun view(source: String, inout aux: String): borrow String from source = source
fun run(): Unit { val owner = "kept"; var holder = Holder("other"); borrow val item = view(owner, &holder.text); println(item) }"#,
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(
            unit.loans()
                .iter()
                .any(|loan| loan.kind() == LoanKind::Exclusive
                    && matches!(loan.target(), UnitLoanTarget::Place(place) if !place.is_root()))
        );
        assert!(unit.borrow_call_handoffs().is_empty());
    }
}

#[test]
fn inout_source_and_synchronous_continuation_still_require_their_own_contract() {
    for text in [
        r#"fun view(inout source: String): borrow String from source = source
fun run(): Unit { var owner = "kept"; borrow val item = view(&owner); println(item) }"#,
        r#"fun view(source: String, inout aux: String): borrow String from source = source
fun read(text: String): Unit {}
fun run(): Unit { val owner = "kept"; var other = "other"; read(view(owner, &other)) }"#,
    ] {
        let observed = observe(text);
        for unit in &observed.units {
            assert!(
                unit.diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().to_string() == "L0164"),
                "{:?}",
                unit.diagnostics()
            );
            assert!(unit.borrow_call_handoffs().is_empty());
            assert!(unit.borrow_call_transports().is_empty());
            assert!(unit.borrow_binding_transports().is_empty());
        }
    }
}

#[test]
fn failure_publication_revokes_proven_exclusive_auxiliary_handoffs() {
    let prefix = r#"fun view(source: String, inout aux: String): borrow String from source = source
fun run(): Unit { val owner = "kept"; var other = "other"; borrow val item = view(owner, &other); println(item)"#;
    assert_handoff(
        &observe(&format!("{prefix} }}")),
        "view(owner, &other)",
        &[("other", "inout aux: String", LoanKind::Exclusive)],
    );
    for (suffix, error) in [
        ("; val moved = owner; println(owner)", true),
        (
            "; var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 })",
            false,
        ),
    ] {
        let observed = observe(&format!("{prefix}{suffix} }}"));
        for unit in &observed.units {
            if error {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|diagnostic| diagnostic.code().to_string() == "L0131")
                );
            } else {
                assert!(unit.diagnostics().is_empty());
                assert!(!unit.deferred().is_empty());
            }
            assert!(unit.borrow_call_handoffs().is_empty());
            assert!(unit.borrow_binding_transports().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn cross_source_exclusive_auxiliary_preserves_call_and_parameter_lineage() {
    let mut sources = SourceMap::new();
    let provider = sources.add_source("provider.ko", "package demo\nfun view(source: String, inout aux: String): borrow String from source = source").unwrap();
    let caller = sources.add_source("caller.ko", r#"package demo
fun run(): Unit { val owner = "kept"; var other = "other"; borrow val item = view(aux = &(other), source = owner); println(item) }"#).unwrap();
    let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
    let caller_file = parse_file(&sources, &lex(&sources, caller).unwrap()).unwrap();
    assert!(provider_file.diagnostics().is_empty());
    assert!(caller_file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let original = [
        SourceUnitInput::new("root", "demo/provider.ko", provider, &provider_file),
        SourceUnitInput::new("root", "demo/caller.ko", caller, &caller_file),
    ];
    for inputs in [original, [original[1], original[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let caller_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == caller)
            .unwrap()
            .id();
        let provider_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == provider)
            .unwrap()
            .id();
        assert_ne!(caller_unit, provider_unit);
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let typed = typed.validate().unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty());
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one real caller handoff");
        };
        let [loan] = handoff.auxiliary_loan_ends() else {
            panic!("one actual auxiliary end");
        };
        assert!(unit.loans().contains(loan));
        assert_eq!(loan.kind(), LoanKind::Exclusive);
        assert_eq!(handoff.source_loan().kind(), LoanKind::Shared);
        assert_eq!(loan.call().source_unit(), caller_unit);
        assert_eq!(loan.argument().source_unit(), caller_unit);
        assert_eq!(handoff.binding().source_unit(), caller_unit);
        assert_eq!(loan.call(), handoff.source_loan().call());
        assert_eq!(handoff.point(), UnitDropPoint::CallReturn(loan.call()));
        let call = typed.types().call(loan.call()).unwrap();
        let UnitCallTarget::Declaration(target) = call.target() else {
            panic!("actual provider");
        };
        let declaration = index
            .declarations()
            .iter()
            .find(|declaration| declaration.id() == target)
            .unwrap();
        assert_eq!(declaration.source_unit(), provider_unit);
        assert_eq!(declaration.name(), "view");
        let parameter = loan.parameter_span().unwrap();
        assert_eq!(parameter.source_id(), provider);
        assert_eq!(sources.slice(parameter), Ok("inout aux: String"));
        assert_eq!(loan.begin_span().source_id(), caller);
        assert_eq!(sources.slice(loan.begin_span()), Ok("&"));
        assert_eq!(loan.end_span().source_id(), caller);
        assert_eq!(
            sources.slice(loan.end_span()),
            Ok("view(aux = &(other), source = owner)")
        );
        let argument = caller_file
            .ast()
            .expressions()
            .get(loan.argument().expression())
            .unwrap();
        assert_eq!(sources.slice(argument.span()), Ok("(other)"));
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("caller owned root");
        };
        assert!(place.is_root());
        assert_eq!(place.root().source_unit(), caller_unit);
        assert!(unit.formed_owned_locals().contains(&place.root()));
        let contract = unit
            .call_argument_contracts_for(loan.call())
            .find(|contract| contract.argument() == loan.argument())
            .unwrap();
        assert_eq!(contract.parameter_index(), 1);
        assert_eq!(contract.parameter_span(), Some(parameter));
        unit.validate().unwrap();
    }
}
