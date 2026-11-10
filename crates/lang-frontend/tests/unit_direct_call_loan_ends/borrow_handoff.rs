//! A normal-return handoff is not a complete loan-end stream.
use super::*;

pub(super) fn assert_handoff_identity(observed: &Observed) {
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
    let (observed, single) = observe_with_legacy(&format!(
        "{VIEW}fun run(): Unit {{ val source = \"kept\"; borrow val item = view(source); println(item) }}"
    ));
    // Bounded Unit observations cannot replace the complete-stream absence witness.
    let old_source = single.borrow_results().bindings()[0].source_loan().unwrap();
    assert!(
        !single
            .loan_ends()
            .iter()
            .any(|end| end.call() == old_source.call() && end.argument() == old_source.argument())
    );
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    super::normal_oracle::assert_normal_handoff(
        &observed,
        (111, 115),
        ((118, 130), (123, 129), (9, 23), (83, 89)),
        &[],
    );
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one handoff")
        };
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
        assert!(unit.borrow_results().bindings().is_empty());
        assert_eq!(
            unit.borrow_call_prefix_ends().len(),
            1,
            "evaluated source unwinds without a final binding"
        );
    }
}

#[test]
fn grouped_initializer_is_observed_while_parent_and_nested_consumers_stay_excluded() {
    for body in [
        "fun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = view(parent); println(child) }",
        "fun observe(text: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; observe(view(owner)) }",
        "fun run(): Unit { val owner = \"kept\"; borrow val item = (view(owner)); println(item) }",
    ] {
        let observed = observe(&format!("{VIEW}{body}"));
        assert_units_admitted(&observed);
        assert_handoff_identity(&observed);
        for unit in &observed.units {
            if body.contains("borrow val parent")
                || body.contains("borrow val item = (view(owner))")
            {
                let [fact] = unit.borrow_call_handoffs() else {
                    panic!("only the original owned-root handoff")
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
                usize::from(
                    body.contains("borrow val parent")
                        || body.contains("borrow val item = (view(owner))")
                )
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

#[test]
fn cross_source_handoffs_keep_caller_and_provider_lineage_under_input_reordering() {
    let mut sources = SourceMap::new();
    let provider = sources
        .add_source(
            "provider.ko",
            "package demo\nfun view(source: String): borrow String from source = source",
        )
        .unwrap();
    let caller = sources.add_source("caller.ko", "package demo\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); println(item) }").unwrap();
    let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
    let caller_file = parse_file(&sources, &lex(&sources, caller).unwrap()).unwrap();
    assert!(provider_file.diagnostics().is_empty());
    assert!(caller_file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    for reverse in [false, true] {
        let mut inputs = [
            SourceUnitInput::new("root", "demo/provider.ko", provider, &provider_file),
            SourceUnitInput::new("root", "demo/caller.ko", caller, &caller_file),
        ];
        if reverse {
            inputs.reverse();
        }
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let caller_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == caller)
            .unwrap();
        let provider_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == provider)
            .unwrap();
        assert_ne!(caller_unit.id(), provider_unit.id());
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one caller handoff")
        };
        let loan = handoff.source_loan();
        assert_eq!(loan.call().source_unit(), caller_unit.id());
        assert_eq!(loan.argument().source_unit(), caller_unit.id());
        assert_eq!(handoff.binding().source_unit(), caller_unit.id());
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("caller owned root")
        };
        assert_eq!(place.root().source_unit(), caller_unit.id());
        let parameter = loan.parameter_span().unwrap();
        assert_eq!(parameter.source_id(), provider);
        assert_eq!(sources.slice(parameter), Ok("source: String"));
        let call = caller_file
            .ast()
            .expressions()
            .get(loan.call().expression())
            .unwrap();
        assert_eq!(sources.slice(call.span()), Ok("view(owner)"));
        let argument = caller_file
            .ast()
            .expressions()
            .get(loan.argument().expression())
            .unwrap();
        assert_eq!(sources.slice(argument.span()), Ok("owner"));
        assert_eq!(handoff.point(), UnitDropPoint::CallReturn(loan.call()));
        unit.validate().unwrap();
    }
}

#[test]
fn sibling_branches_commit_only_their_own_binding_identity() {
    let observed = observe(&format!(
        "{VIEW}fun run(flag: Boolean): Unit {{ val owner = \"kept\"; if (flag) {{ borrow val first = view(owner); println(first) }} else {{ borrow val second = view(owner); println(second) }} }}"
    ));
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    for unit in &observed.units {
        let [first, second] = unit.borrow_call_handoffs() else {
            panic!("two distinct branch handoffs")
        };
        assert_ne!(first.binding(), second.binding());
        assert_ne!(first.source_loan().call(), second.source_loan().call());
        for handoff in [first, second] {
            let matching: Vec<_> = unit
                .borrow_results()
                .bindings()
                .iter()
                .filter(|binding| binding.binding() == handoff.binding())
                .collect();
            let [binding] = matching.as_slice() else {
                panic!("one matching established binding")
            };
            assert_eq!(binding.initializer(), handoff.source_loan().call());
            assert_eq!(
                binding.source_loan().unwrap().argument(),
                handoff.source_loan().argument()
            );
        }
    }
}

#[test]
fn return_during_later_operand_ends_only_the_established_source_prefix() {
    let observed = observe(
        "fun select(source: String, other: String): borrow String from source = source\nfun run(flag: Boolean): Unit { val owner = \"kept\"; val other = \"other\"; borrow val item = select(owner, if (flag) { return } else { other }); println(item) }",
    );
    assert_units_admitted(&observed);
    super::prefix_oracle::assert_prefix_identities(
        &observed,
        &[((168, 218), (175, 180), (11, 25), (113, 118), (194, 200))],
    );
    assert_handoff_identity(&observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source")
        };
        let [event] = unit.borrow_call_prefix_ends() else {
            panic!("one actual abandoned source token")
        };
        assert_eq!(event.loan().call().source_unit(), indexed.id());
        assert_eq!(event.loan().argument().source_unit(), indexed.id());
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one binding on the continuing edge")
        };
        let [local] = names.names().source_units() else {
            panic!("same-round resolution")
        };
        assert_eq!(binding.binding().source_unit(), indexed.id());
        let symbol = &local.resolution().symbols()[binding.binding().symbol().index()];
        assert_eq!(symbol.id(), binding.binding().symbol());
        assert_eq!(symbol.span().source_id(), observed.parsed.source_id());
        assert_eq!((symbol.span().start(), symbol.span().end()), (161, 165));
        assert_eq!(binding.initializer(), event.loan().call());
        assert_eq!(binding.origin(), event.loan().target());
        assert_eq!(event.preceding_call_loan_ends(), 0);
        let [result_end] = unit.borrow_results().ends() else {
            panic!("only the established result has an endpoint")
        };
        assert_eq!(result_end.binding(), binding.binding());
        assert!(matches!(
            result_end.point(),
            UnitDropPoint::AfterStatement(_)
        ));
        assert_ne!(result_end.point(), event.point());
        let source = binding.source_loan().unwrap();
        assert_eq!(event.loan().call(), source.call());
        assert_eq!(event.loan().argument(), source.argument());
        let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
            panic!("actual return")
        };
        assert_eq!(transfer.source_unit(), indexed.id());
        let node = observed
            .parsed
            .ast()
            .expressions()
            .get(transfer.expression())
            .unwrap();
        assert_eq!(observed.sources.slice(node.span()), Ok("return"));
        let UnitLoanTarget::Place(place) = event.loan().target() else {
            panic!("root source")
        };
        assert_eq!(place.root().source_unit(), indexed.id());
        assert!(place.is_root());
        assert!(unit.formed_owned_locals().contains(&place.root()));
        assert_eq!(event.loan().kind(), LoanKind::Shared);
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.point() == event.point())
            .collect();
        let owner_drop = drops
            .iter()
            .position(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .unwrap();
        assert!(event.preceding_drops() <= owner_drop);
        assert_eq!(event.preceding_drops(), 0);
        assert!(unit.loans().contains(event.loan()));
    }
}

#[test]
fn source_prefix_ends_require_evaluation_before_the_return() {
    for (arguments, expected) in [
        (
            "other = owner, source = owner, tail = if (flag) { return } else { \"tail\" }",
            1,
        ),
        (
            "tail = if (flag) { return } else { \"tail\" }, other = owner, source = owner",
            0,
        ),
    ] {
        let observed = observe(&format!(
            "fun select(other: String, source: String, tail: String): borrow String from source = source\nfun run(flag: Boolean): Unit {{ val owner = \"kept\"; borrow val item = select({arguments}); println(item) }}"
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert_eq!(unit.borrow_call_prefix_ends().len(), expected);
            let source = unit.borrow_results().bindings()[0].source_loan().unwrap();
            for end in unit.borrow_call_prefix_ends() {
                assert_eq!(end.loan().argument(), source.argument());
                assert_eq!(end.loan().call(), source.call());
                assert_eq!(
                    observed.sources.slice(end.loan().parameter_span().unwrap()),
                    Ok("source: String")
                );
            }
        }
    }
}

#[test]
fn completed_handoff_is_not_an_abandoned_call_prefix() {
    let observed = observe(&format!(
        "{VIEW}fun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); println(item); return }}"
    ));
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    for unit in &observed.units {
        assert_eq!(unit.borrow_call_handoffs().len(), 1);
        assert!(unit.borrow_call_prefix_ends().is_empty());
    }
}

#[test]
fn errors_and_deferred_work_revoke_nonempty_source_prefix_observations() {
    let prefix = "fun select(source: String, other: String): borrow String from source = source\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = select(owner, if (flag) { return } else { \"tail\" }); println(item)";
    let good = observe(&format!("{prefix} }}"));
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.borrow_call_prefix_ends().len(), 1);
    }
    for (suffix, error) in [
        ("; val bad = \"bad\"; val moved = bad; println(bad)", true),
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
                        .any(|d| d.code().to_string() == "L0131"),
                    "{:?}",
                    unit.diagnostics()
                );
            } else {
                assert!(unit.diagnostics().is_empty());
                assert!(!unit.deferred().is_empty());
            }
            assert!(unit.borrow_call_prefix_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn unconditional_named_return_keeps_actual_source_identity_without_a_binding() {
    let observed = observe(
        "fun select(other: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(source = owner, other = return) }",
    );
    assert_units_admitted(&observed);
    super::prefix_oracle::assert_return_prefixes(
        &observed,
        &[((134, 172), (150, 155), (26, 40), (100, 105), (165, 171))],
    );
    assert_handoff_identity(&observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_call_handoffs().is_empty());
        let [event] = unit.borrow_call_prefix_ends() else {
            panic!("one actual source prefix")
        };
        let [indexed] = names.names().index().source_units() else {
            panic!("one source")
        };
        assert_eq!(event.loan().call().source_unit(), indexed.id());
        assert_eq!(event.loan().argument().source_unit(), indexed.id());
        let call = observed
            .parsed
            .ast()
            .expressions()
            .get(event.loan().call().expression())
            .unwrap();
        let argument = observed
            .parsed
            .ast()
            .expressions()
            .get(event.loan().argument().expression())
            .unwrap();
        assert_eq!(
            observed.sources.slice(call.span()),
            Ok("select(source = owner, other = return)")
        );
        assert_eq!(observed.sources.slice(argument.span()), Ok("owner"));
        assert_eq!(
            observed
                .sources
                .slice(event.loan().parameter_span().unwrap()),
            Ok("source: String")
        );
        let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
            panic!("return")
        };
        assert_eq!(transfer.source_unit(), indexed.id());
        let UnitLoanTarget::Place(place) = event.loan().target() else {
            panic!("owned root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.point() == event.point())
            .collect();
        let position = drops
            .iter()
            .position(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .unwrap();
        assert!(event.preceding_drops() <= position);
    }
}

#[test]
fn unconditional_return_before_source_does_not_invent_a_token() {
    let observed = observe(
        "fun select(other: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(other = return, source = owner) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.borrow_call_prefix_ends().is_empty());
    }
}

#[test]
fn parent_operand_cannot_masquerade_as_its_canonical_owned_root() {
    let observed = observe(&format!(
        "{VIEW}fun select(source: String, other: String): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; borrow val parent = view(owner); borrow val child = select(parent, return) }}"
    ));
    assert_units_admitted(&observed);
    assert_handoff_identity(&observed);
    for unit in &observed.units {
        assert_eq!(unit.borrow_call_handoffs().len(), 1);
        assert!(unit.borrow_call_prefix_ends().is_empty());
        assert!(
            unit.loans().iter().any(|loan| {
                let argument = observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                    .unwrap();
                observed.sources.slice(argument.span()) == Ok("parent")
            }),
            "the excluded parent actually produced a loan"
        );
    }
}

#[test]
fn cross_source_prefix_ends_keep_caller_return_and_provider_parameter_lineage() {
    let mut sources = SourceMap::new();
    let provider = sources
        .add_source(
            "provider.ko",
            "package demo\nfun view(source: String, tail: String): borrow String from source = source",
        )
        .unwrap();
    let caller = sources.add_source("caller.ko", "package demo\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, return) }").unwrap();
    let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
    let caller_file = parse_file(&sources, &lex(&sources, caller).unwrap()).unwrap();
    assert!(provider_file.diagnostics().is_empty());
    assert!(caller_file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    for reverse in [false, true] {
        let mut inputs = [
            SourceUnitInput::new("root", "demo/provider.ko", provider, &provider_file),
            SourceUnitInput::new("root", "demo/caller.ko", caller, &caller_file),
        ];
        if reverse {
            inputs.reverse();
        }
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let caller_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == caller)
            .unwrap();
        let provider_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == provider)
            .unwrap();
        assert_ne!(caller_unit.id(), provider_unit.id());
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.borrow_results().bindings().is_empty());
        let [event] = unit.borrow_call_prefix_ends() else {
            panic!("one caller prefix end")
        };
        let loan = event.loan();
        assert_eq!(loan.call().source_unit(), caller_unit.id());
        assert_eq!(loan.argument().source_unit(), caller_unit.id());
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("caller owned root")
        };
        assert_eq!(place.root().source_unit(), caller_unit.id());
        let parameter = loan.parameter_span().unwrap();
        assert_eq!(parameter.source_id(), provider);
        assert_eq!(sources.slice(parameter), Ok("source: String"));
        let call = caller_file
            .ast()
            .expressions()
            .get(loan.call().expression())
            .unwrap();
        assert_eq!(sources.slice(call.span()), Ok("view(owner, return)"));
        let argument = caller_file
            .ast()
            .expressions()
            .get(loan.argument().expression())
            .unwrap();
        assert_eq!(sources.slice(argument.span()), Ok("owner"));
        let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
            panic!("actual return")
        };
        assert_eq!(transfer.source_unit(), caller_unit.id());
        let transfer_node = caller_file
            .ast()
            .expressions()
            .get(transfer.expression())
            .unwrap();
        assert_eq!(transfer_node.span().source_id(), caller);
        assert_eq!(sources.slice(transfer_node.span()), Ok("return"));
        assert!(unit.loans().contains(loan));
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.point() == event.point())
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].target(), UnitDropTarget::Named(place.root()));
        assert_eq!(event.preceding_drops(), 0);
        unit.validate().unwrap();
    }
}

#[test]
fn sibling_return_edges_keep_distinct_source_tokens_and_cleanup_points() {
    let observed = observe(
        "fun select(source: String, tail: String): borrow String from source = source\nfun run(flag: Boolean): Unit { val owner = \"kept\"; if (flag) { borrow val first = select(owner, return) } else { borrow val second = select(owner, return) } }",
    );
    assert_units_admitted(&observed);
    super::prefix_oracle::assert_return_prefixes(
        &observed,
        &[
            ((159, 180), (166, 171), (11, 25), (112, 117), (173, 179)),
            ((210, 231), (217, 222), (11, 25), (112, 117), (224, 230)),
        ],
    );
    for unit in &observed.units {
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_call_handoffs().is_empty());
        let [first, second] = unit.borrow_call_prefix_ends() else {
            panic!("one event per static sibling return edge")
        };
        assert_ne!(first.loan().call(), second.loan().call());
        assert_ne!(first.loan().argument(), second.loan().argument());
        assert_ne!(first.point(), second.point());
        assert_eq!(first.preceding_call_loan_ends(), 0);
        assert_eq!(second.preceding_call_loan_ends(), 0);
        assert_eq!(first.loan().target(), second.loan().target());
        for event in [first, second] {
            let UnitLoanTarget::Place(place) = event.loan().target() else {
                panic!("owned root")
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.point() == event.point())
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].target(), UnitDropTarget::Named(place.root()));
            assert_eq!(event.preceding_drops(), 0);
        }
    }
}

#[test]
fn nested_auxiliary_return_preserves_source_end_and_temporary_cleanup() {
    let observed = observe(
        "fun select(source: String, tail: Unit): borrow String from source = source\nfun sink(value: String, stop: Unit): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, sink(\"temporary\", return)) }",
    );
    assert_units_admitted(&observed);
    super::prefix_oracle::assert_prefix_identities(
        &observed,
        &[((176, 216), (183, 188), (11, 25), (142, 147), (208, 214))],
    );
    assert_fixed_ends(
        &observed,
        &[ExpectedEnd {
            call: (190, 215),
            argument: (195, 206),
            parameter: (84, 97),
            point: ExpectedPoint::Return((208, 214)),
            target: ExpectedTarget::Temporary,
        }],
    );
    let expression = |text| {
        let found: Vec<_> = observed
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                (observed.sources.slice(node.span()) == Ok(text)).then_some(id)
            })
            .collect();
        let [id] = found.as_slice() else {
            panic!("unique source expression: {text}")
        };
        *id
    };
    let outer = expression("select(owner, sink(\"temporary\", return))");
    let inner = expression("sink(\"temporary\", return)");
    let temporary = expression("\"temporary\"");
    let source = expression("owner");
    let transfer = expression("return");
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source")
        };
        let qualify = |id| UnitExpressionId::new(indexed.id(), id);
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_call_handoffs().is_empty());
        let [event] = unit.borrow_call_prefix_ends() else {
            panic!("one actual source end")
        };
        assert_eq!(event.loan().call(), qualify(outer));
        assert_eq!(event.loan().argument(), qualify(source));
        assert_eq!(
            event.point(),
            UnitDropPoint::ControlTransfer(qualify(transfer))
        );
        assert_eq!(event.preceding_drops(), 0);
        let UnitLoanTarget::Place(owner) = event.loan().target() else {
            panic!("owned root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.point() == event.point())
            .map(|drop| drop.target())
            .collect();
        assert_eq!(
            drops,
            vec![
                UnitDropTarget::Temporary(qualify(temporary)),
                UnitDropTarget::Named(owner.root())
            ]
        );
        let [auxiliary] = unit.direct_call_loan_ends() else {
            panic!("one actual auxiliary end, separate from the borrow source slice")
        };
        assert_eq!(auxiliary.loan().call(), qualify(inner));
        assert_eq!(auxiliary.loan().argument(), qualify(temporary));
        assert_eq!(
            auxiliary.loan().target(),
            &UnitLoanTarget::Temporary(qualify(temporary))
        );
        assert_eq!(auxiliary.point(), event.point());
        assert_eq!(auxiliary.preceding_drops(), 0);
        assert_eq!(auxiliary.loan().kind(), LoanKind::Shared);
        assert_eq!(event.loan().kind(), LoanKind::Shared);
        assert!(unit.loans().contains(event.loan()));
        assert!(unit.loans().contains(auxiliary.loan()));
        // Same-point producer ranks prove auxiliary-before-source without a global order.
        assert_eq!(auxiliary.preceding_call_loan_ends(), 0);
        assert_eq!(event.preceding_call_loan_ends(), 1);
    }
}
