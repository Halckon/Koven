//! Actual auxiliary token retirement is distinct from the continuing borrow source.
use super::*;

#[test]
fn normal_handoff_ends_auxiliary_tokens_in_source_order() {
    let (observed, legacy) = observe_with_legacy(
        "fun select(first: String, source: String, last: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; val first = \"first\"; val last = \"last\"; borrow val item = select(last = last, source = owner, first = first); println(item) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one committed handoff")
        };
        let ended = handoff.auxiliary_loan_ends();
        assert_eq!(ended.len(), 2);
        let text: Vec<_> = ended
            .iter()
            .map(|loan| {
                let node = observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                    .unwrap();
                observed.sources.slice(node.span()).unwrap()
            })
            .collect();
        assert_eq!(text, ["last", "first"]);
        for loan in ended {
            assert!(unit.loans().contains(loan));
            assert_eq!(loan.call(), handoff.source_loan().call());
            assert_ne!(loan.argument(), handoff.source_loan().argument());
            assert_eq!(loan.kind(), LoanKind::Shared);
        }
        let old: Vec<_> = legacy
            .loan_ends()
            .iter()
            .filter(|end| end.call() == handoff.source_loan().call().expression())
            .collect();
        assert_eq!(old.len(), 2);
        for (new, old) in ended.iter().zip(old) {
            assert_eq!(new.argument().expression(), old.argument());
            assert_eq!(
                old.point(),
                lang_frontend::ownership_checking::LoanEndPoint::CallReturn(
                    new.call().expression()
                )
            );
        }
    }
}

#[test]
fn shared_root_identity_does_not_end_the_continuing_source_argument() {
    let observed = observe(
        "fun select(aux: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(source = owner, aux = (owner)); println(item) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one handoff")
        };
        let [aux] = handoff.auxiliary_loan_ends() else {
            panic!("one auxiliary")
        };
        let source = handoff.source_loan();
        assert_eq!(aux.call(), source.call());
        assert_ne!(aux.argument(), source.argument());
        assert_eq!(aux.target(), source.target());
        assert_eq!(
            observed.sources.slice(aux.parameter_span().unwrap()),
            Ok("aux: String")
        );
        assert_eq!(
            observed.sources.slice(source.parameter_span().unwrap()),
            Ok("source: String")
        );
        let UnitLoanTarget::Place(origin) = source.target() else {
            panic!("owned root")
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
fn borrowed_auxiliary_is_not_misreported_as_owned_root_retirement() {
    let observed = observe(
        "fun select(aux: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; val other = \"other\"; borrow val alias = other; borrow val item = select(alias, owner); println(item); println(alias) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("source handoff preserved")
        };
        assert!(handoff.auxiliary_loan_ends().is_empty());
        assert_eq!(
            unit.loans()
                .iter()
                .filter(|loan| loan.call() == handoff.source_loan().call())
                .count(),
            2
        );
    }
}

#[test]
fn source_only_and_abandoned_calls_do_not_invent_auxiliary_ends() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); println(item) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one handoff")
        };
        assert!(handoff.auxiliary_loan_ends().is_empty());
    }
    let observed = observe(
        "fun select(aux: String, source: String, stop: Unit): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; val aux = \"aux\"; borrow val item = select(aux, owner, return) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(unit.borrow_call_handoffs().is_empty());
    }
}

#[test]
fn later_errors_and_deferred_work_revoke_nonempty_auxiliary_ends() {
    let good = "fun view(aux: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; val aux = \"aux\"; borrow val item = view(aux, owner); println(item)";
    let baseline = observe(&format!("{good} }}"));
    assert_units_admitted(&baseline);
    for unit in &baseline.units {
        assert_eq!(
            unit.borrow_call_handoffs()[0].auxiliary_loan_ends().len(),
            1
        );
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
fn cross_source_auxiliary_ends_keep_caller_and_provider_lineage() {
    let mut sources = SourceMap::new();
    let provider = sources
        .add_source(
            "provider.ko",
            "package demo\nfun view(source: String, aux: String): borrow String from source = source",
        )
        .unwrap();
    let caller = sources.add_source("caller.ko", "package demo\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); println(item) }").unwrap();
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
        let [loan] = handoff.auxiliary_loan_ends() else {
            panic!("one auxiliary end")
        };
        assert_ne!(loan.argument(), handoff.source_loan().argument());
        assert_eq!(loan.call().source_unit(), caller_unit.id());
        assert_eq!(loan.argument().source_unit(), caller_unit.id());
        assert_eq!(handoff.binding().source_unit(), caller_unit.id());
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("caller owned root")
        };
        assert_eq!(place.root().source_unit(), caller_unit.id());
        let parameter = loan.parameter_span().unwrap();
        assert_eq!(parameter.source_id(), provider);
        assert_eq!(sources.slice(parameter), Ok("aux: String"));
        let call = caller_file
            .ast()
            .expressions()
            .get(loan.call().expression())
            .unwrap();
        assert_eq!(sources.slice(call.span()), Ok("view(owner, owner)"));
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
fn nested_borrow_result_auxiliary_keeps_its_separate_end_contract() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun select(aux: String, source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; val aux = \"aux\"; borrow val item = select(view(aux), owner); println(item) }",
    );
    for unit in &observed.units {
        assert!(
            unit.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0164")
        );
        assert!(unit.borrow_call_handoffs().is_empty());
    }
}
