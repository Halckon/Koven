//! A completed auxiliary temporary retires at the call, not at the borrowed result end.
use super::*;

#[test]
fn temporary_auxiliary_retires_without_ending_the_borrow_source() {
    let observed = observe(
        "fun select(source: String, aux: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, \"temporary\"); println(item) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one established binding");
        };
        let [transport] = unit.borrow_binding_transports() else {
            panic!("existing source token transport");
        };
        assert_eq!(transport.binding(), binding.binding());
        let actual: Vec<_> = unit
            .loans()
            .iter()
            .filter(|loan| matches!(loan.target(), UnitLoanTarget::Temporary(_)))
            .collect();
        let [temporary] = actual.as_slice() else {
            panic!("one already-evaluated auxiliary temporary");
        };
        assert_eq!(
            temporary.target(),
            &UnitLoanTarget::Temporary(temporary.argument())
        );
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(temporary.argument()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(
            drops[0].point(),
            UnitDropPoint::CallReturn(temporary.call())
        );
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one handoff from existing evaluated source and temporary");
        };
        assert_eq!(handoff.source_loan(), transport.source_loan());
        assert_eq!(handoff.auxiliary_loan_ends(), [(*temporary).clone()]);
        assert_ne!(handoff.point(), transport.point());
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TargetBytes {
    Root(ByteSpan),
    Temporary(ByteSpan),
}

fn span(text: &str, marker: &str) -> ByteSpan {
    let found: Vec<_> = text.match_indices(marker).map(|(start, _)| start).collect();
    let [start] = found.as_slice() else {
        panic!("unique marker {marker:?}: {found:?}");
    };
    (*start, start + marker.len())
}

fn assert_completed(
    observed: &Observed,
    text: &str,
    call: &str,
    auxiliary: &[(&str, &str, Option<&str>)],
) {
    assert_units_admitted(observed);
    super::borrow_handoff::assert_handoff_identity(observed);
    let call_bytes = span(text, call);
    let argument_bytes = |marker: &str| {
        let (start, end) = span(call, marker);
        let prefix = marker.find(" = ").map_or(0, |index| index + 3);
        (call_bytes.0 + start + prefix, call_bytes.0 + end)
    };
    let root_bytes = |name: &str| {
        let (start, _) = span(text, &format!("val {name} ="));
        (start + 4, start + 4 + name.len())
    };
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source")
        };
        let [local] = names.names().source_units() else {
            panic!("same-round symbols")
        };
        let bytes = |span: Span| {
            assert_eq!(span.source_id(), observed.parsed.source_id());
            (span.start(), span.end())
        };
        let expression_bytes = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), indexed.id());
            bytes(
                observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(id.expression())
                    .unwrap()
                    .span(),
            )
        };
        let normalize = |loan: &lang_frontend::ownership_checking::UnitLoanFact| {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert!(unit.loans().contains(loan));
            assert_eq!(bytes(loan.begin_span()), expression_bytes(loan.argument()));
            assert_eq!(bytes(loan.end_span()), expression_bytes(loan.call()));
            let target = match loan.target() {
                UnitLoanTarget::Place(place) => {
                    assert!(place.is_root());
                    assert_eq!(place.root().source_unit(), indexed.id());
                    assert!(unit.formed_owned_locals().contains(&place.root()));
                    TargetBytes::Root(bytes(
                        local.resolution().symbols()[place.root().symbol().index()].span(),
                    ))
                }
                UnitLoanTarget::Temporary(owner) => {
                    assert_eq!(*owner, loan.argument());
                    TargetBytes::Temporary(expression_bytes(*owner))
                }
                other => panic!("bounded owned auxiliary target: {other:?}"),
            };
            (
                expression_bytes(loan.call()),
                expression_bytes(loan.argument()),
                bytes(loan.parameter_span().unwrap()),
                target,
            )
        };
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("complete singleton handoff table")
        };
        assert_eq!(
            normalize(handoff.source_loan()),
            (
                call_bytes,
                argument_bytes("owner"),
                span(text, "source: String"),
                TargetBytes::Root(root_bytes("owner"))
            )
        );
        let expected: Vec<_> = auxiliary
            .iter()
            .map(|(argument, parameter, root)| {
                (
                    call_bytes,
                    argument_bytes(argument),
                    span(text, parameter),
                    root.map_or_else(
                        || TargetBytes::Temporary(argument_bytes(argument)),
                        |root| TargetBytes::Root(root_bytes(root)),
                    ),
                )
            })
            .collect();
        assert_eq!(
            handoff
                .auxiliary_loan_ends()
                .iter()
                .map(normalize)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            handoff.point(),
            UnitDropPoint::CallReturn(handoff.source_loan().call())
        );
        assert!(
            handoff
                .auxiliary_loan_ends()
                .iter()
                .all(|loan| loan.argument() != handoff.source_loan().argument())
        );
        assert!(
            unit.direct_call_loan_ends()
                .iter()
                .all(|end| end.loan().call() != handoff.source_loan().call())
        );
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one committed binding")
        };
        assert_eq!(binding.binding(), handoff.binding());
        assert_eq!(binding.initializer(), handoff.source_loan().call());
        assert_eq!(binding.origin(), handoff.source_loan().target());
        let [transport] = unit.borrow_binding_transports() else {
            panic!("original source transport")
        };
        assert_eq!(transport.binding(), handoff.binding());
        assert_eq!(transport.source_loan(), handoff.source_loan());
        let [end] = unit.borrow_results().ends() else {
            panic!("original result endpoint")
        };
        assert_eq!(transport.point(), end.point());
        assert_ne!(transport.point(), handoff.point());
        let temporary_drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.point() == handoff.point())
            .filter_map(|drop| match drop.target() {
                UnitDropTarget::Temporary(owner) => Some(expression_bytes(owner)),
                _ => None,
            })
            .collect();
        let expected_drops: Vec<_> = auxiliary
            .iter()
            .rev()
            .filter(|(_, _, root)| root.is_none())
            .map(|(argument, _, _)| argument_bytes(argument))
            .collect();
        assert_eq!(temporary_drops, expected_drops);
        let UnitLoanTarget::Place(source) = handoff.source_loan().target() else {
            panic!("continuing root")
        };
        assert!(
            !unit
                .drops()
                .iter()
                .any(|drop| drop.point() == handoff.point()
                    && drop.target() == UnitDropTarget::Named(source.root()))
        );
        assert!(unit.drops().iter().any(|drop| drop.point() == end.point()
            && drop.target() == UnitDropTarget::Named(source.root())));
    }
}

#[test]
fn completed_owned_temporary_auxiliaries_use_the_actual_registered_owner() {
    for operand in [
        "\"text\"",
        "((\"grouped\"))",
        "\"value ${1}\"",
        "\"left\" + \"right\"",
        "make()",
    ] {
        let call = format!("select(owner, {operand})");
        let text = format!(
            "fun make(): String = \"made\"\nfun select(source: String, aux: String): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = {call}; println(item) }}"
        );
        let observed = observe(&text);
        assert_completed(&observed, &text, &call, &[(operand, "aux: String", None)]);
    }
    let text = "class Node {}\nfun select(source: String, aux: Node): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, Node()); println(item) }";
    assert_completed(
        &observe(text),
        text,
        "select(owner, Node())",
        &[("Node()", "aux: Node", None)],
    );
}

#[test]
fn mixed_named_auxiliaries_preserve_pending_order_and_reverse_temporary_cleanup() {
    let call = "select(last = \"last\", source = owner, other = other, first = ((\"first\")))";
    let text = format!(
        "fun select(first: String, source: String, other: String, last: String): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; val other = \"named\"; borrow val item = {call}; println(item) }}"
    );
    assert_completed(
        &observe(&text),
        &text,
        call,
        &[
            ("\"last\"", "last: String", None),
            ("other = other", "other: String", Some("other")),
            ("((\"first\"))", "first: String", None),
        ],
    );
}

#[test]
fn mixed_return_keeps_only_the_completed_temporary_on_the_continuing_edge() {
    let call = "select(owner, if (flag) { return } else { \"tail\" })";
    let text = format!(
        "fun select(source: String, aux: String): borrow String from source = source\nfun run(flag: Boolean): Unit {{ val owner = \"kept\"; borrow val item = {call}; println(item) }}"
    );
    let observed = observe(&text);
    assert_completed(
        &observed,
        &text,
        call,
        &[(
            "if (flag) { return } else { \"tail\" }",
            "aux: String",
            None,
        )],
    );
    for unit in &observed.units {
        let [prefix] = unit.borrow_call_prefix_ends() else {
            panic!("only the evaluated source unwinds")
        };
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("only the normal edge initializes")
        };
        assert_eq!(prefix.loan(), handoff.source_loan());
        assert_ne!(prefix.point(), handoff.point());
        assert_eq!(prefix.preceding_call_loan_ends(), 0);
        assert!(
            !unit
                .drops()
                .iter()
                .any(|drop| drop.point() == prefix.point()
                    && matches!(drop.target(), UnitDropTarget::Temporary(_)))
        );
    }
}

#[test]
fn abandoned_temporary_auxiliary_does_not_publish_a_normal_handoff() {
    for (arguments, source_evaluated) in [
        ("source = owner, aux = \"ready\", stop = return", true),
        ("aux = \"ready\", stop = return, source = owner", false),
    ] {
        let text = format!(
            "fun select(source: String, aux: String, stop: Unit): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = select({arguments}) }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(unit.borrow_call_handoffs().is_empty());
            assert!(unit.borrow_binding_transports().is_empty());
            assert!(unit.borrow_results().bindings().is_empty());
            assert_eq!(
                unit.borrow_call_prefix_ends().len(),
                usize::from(source_evaluated)
            );
            let loans: Vec<_> = unit
                .loans()
                .iter()
                .filter(|loan| matches!(loan.target(), UnitLoanTarget::Temporary(_)))
                .collect();
            let [temporary] = loans.as_slice() else {
                panic!("one evaluated prefix temporary")
            };
            assert_eq!(
                observed.sources.slice(temporary.begin_span()),
                Ok("\"ready\"")
            );
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(temporary.argument()))
                .collect();
            let [drop] = drops.as_slice() else {
                panic!("one actual unwind drop")
            };
            let UnitDropPoint::ControlTransfer(point) = drop.point() else {
                panic!("actual Return")
            };
            assert_eq!(
                observed.sources.slice(
                    observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(point.expression())
                        .unwrap()
                        .span()
                ),
                Ok("return")
            );
        }
    }
}

#[test]
fn value_arguments_and_temporary_parent_fields_are_not_auxiliary_borrow_owners() {
    let own = "fun select(source: String, own aux: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, \"delivered\"); println(item) }";
    assert_completed(&observe(own), own, "select(owner, \"delivered\")", &[]);
    let field = observe(
        "class Holder(val text: String)\nfun select(source: String, aux: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, Holder(\"field\").text); println(item) }",
    );
    assert_units_admitted(&field);
    for unit in &field.units {
        let [binding] = unit.borrow_results().bindings() else {
            panic!("original binding")
        };
        assert_eq!(unit.borrow_binding_transports().len(), 1);
        assert!(
            unit.borrow_call_handoffs().is_empty(),
            "a parent is not the argument's owned temporary"
        );
        let field_loans: Vec<_> = unit
            .loans()
            .iter()
            .filter(|loan| {
                loan.call() == binding.initializer()
                    && matches!(loan.target(), UnitLoanTarget::Temporary(_))
            })
            .collect();
        let [loan] = field_loans.as_slice() else {
            panic!("actual temporary parent token")
        };
        assert_ne!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
    }
}

#[test]
fn later_errors_and_deferred_work_revoke_temporary_auxiliary_observations() {
    let prefix = "fun select(source: String, aux: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = select(owner, \"aux\"); println(item)";
    let good = observe(&format!("{prefix} }}"));
    assert_units_admitted(&good);
    for unit in &good.units {
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
        let observed = observe(&format!("{prefix}{suffix} }}"));
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
            assert!(unit.borrow_binding_transports().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn cross_source_temporary_auxiliaries_keep_provider_parameters_under_input_reordering() {
    let mut sources = SourceMap::new();
    let provider_text = "package demo\nfun select(first: String, source: String, last: String): borrow String from source = source";
    let call_text = "select(last = \"later\", source = owner, first = ((\"earlier\")))";
    let caller_text = format!(
        "package demo\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = {call_text}; println(item) }}"
    );
    let provider = sources.add_source("provider.ko", provider_text).unwrap();
    let caller = sources.add_source("caller.ko", &caller_text).unwrap();
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
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let typed = raw.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let unit = owned.ownership();
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
        let expression_bytes = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), caller_unit);
            let value = caller_file
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span();
            assert_eq!(value.source_id(), caller);
            (value.start(), value.end())
        };
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one caller handoff")
        };
        let call = handoff.source_loan().call();
        assert_eq!(expression_bytes(call), span(&caller_text, call_text));
        assert_eq!(handoff.binding().source_unit(), caller_unit);
        assert_eq!(
            handoff.source_loan().parameter_span().unwrap().source_id(),
            provider
        );
        assert_eq!(
            sources.slice(handoff.source_loan().parameter_span().unwrap()),
            Ok("source: String")
        );
        let UnitLoanTarget::Place(place) = handoff.source_loan().target() else {
            panic!("caller source")
        };
        assert_eq!(place.root().source_unit(), caller_unit);
        let actual: Vec<_> = handoff
            .auxiliary_loan_ends()
            .iter()
            .map(|loan| {
                assert!(unit.loans().contains(loan));
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert_eq!(loan.call(), call);
                assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
                let parameter = loan.parameter_span().unwrap();
                assert_eq!(parameter.source_id(), provider);
                (
                    expression_bytes(loan.argument()),
                    (parameter.start(), parameter.end()),
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                (
                    span(&caller_text, "\"later\""),
                    span(provider_text, "last: String")
                ),
                (
                    span(&caller_text, "((\"earlier\"))"),
                    span(provider_text, "first: String")
                ),
            ]
        );
        let temporary_drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.point() == handoff.point())
            .filter_map(|drop| match drop.target() {
                UnitDropTarget::Temporary(owner) => Some(expression_bytes(owner)),
                _ => None,
            })
            .collect();
        assert_eq!(
            temporary_drops,
            [
                span(&caller_text, "((\"earlier\"))"),
                span(&caller_text, "\"later\"")
            ]
        );
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one source transport")
        };
        assert_eq!(transport.source_loan(), handoff.source_loan());
        assert_eq!(transport.binding(), handoff.binding());
        assert_ne!(transport.point(), handoff.point());
    }
}

#[test]
fn original_conditional_name_witness_keeps_its_prefix_and_gains_a_normal_handoff() {
    // Keep the pre-existing mixed-return fixture byte-for-byte.
    let text = "fun select(source: String, other: String): borrow String from source = source\nfun run(flag: Boolean): Unit { val owner = \"kept\"; val other = \"other\"; borrow val item = select(owner, if (flag) { return } else { other }); println(item) }";
    let observed = observe(text);
    assert_completed(
        &observed,
        text,
        "select(owner, if (flag) { return } else { other })",
        &[("if (flag) { return } else { other }", "other: String", None)],
    );
    super::prefix_oracle::assert_prefix_identities(
        &observed,
        &[((168, 218), (175, 180), (11, 25), (113, 118), (194, 200))],
    );
    for unit in &observed.units {
        assert_eq!(
            unit.borrow_call_prefix_ends()[0].loan(),
            unit.borrow_call_handoffs()[0].source_loan()
        );
    }
}
