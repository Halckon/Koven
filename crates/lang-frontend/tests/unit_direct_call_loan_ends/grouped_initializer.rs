//! Group syntax preserves the initializer identity while the actual call owns its token.
use super::*;

fn span(text: &str, marker: &str) -> ByteSpan {
    let positions: Vec<_> = text.match_indices(marker).map(|(start, _)| start).collect();
    let [start] = positions.as_slice() else {
        panic!("unique {marker:?}: {positions:?}");
    };
    (*start, *start + marker.len())
}

fn declaration(text: &str, keyword: &str, name: &str) -> ByteSpan {
    let start = span(text, &format!("{keyword} {name} =")).0 + keyword.len() + 1;
    (start, start + name.len())
}

fn loan_bytes(
    text: &str,
    call: &str,
    argument: &str,
    parameter: &str,
    root: &str,
) -> (ByteSpan, ByteSpan, ByteSpan, ByteSpan) {
    let call_span = span(text, call);
    let argument = span(call, argument);
    (
        call_span,
        (call_span.0 + argument.0, call_span.0 + argument.1),
        span(text, parameter),
        declaration(text, "val", root),
    )
}

fn point_text(observed: &Observed, point: UnitDropPoint) -> &str {
    let span = match point {
        UnitDropPoint::AfterStatement(statement) => observed
            .parsed
            .ast()
            .statements()
            .get(statement.statement())
            .unwrap()
            .span(),
        UnitDropPoint::ControlTransfer(expression) => observed
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .unwrap()
            .span(),
        _ => panic!("actual binding termination point"),
    };
    assert_eq!(span.source_id(), observed.parsed.source_id());
    observed.sources.slice(span).unwrap()
}

fn assert_transport(observed: &Observed, initializer: &str, call: &str, end_text: &str) {
    for unit in &observed.units {
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one already-admitted grouped binding");
        };
        let source = binding
            .source_loan()
            .expect("existing source loan identity");
        assert_ne!(binding.initializer(), source.call());
        let expression_text = |id: UnitExpressionId| {
            let node = observed
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap();
            assert_eq!(node.span().source_id(), observed.parsed.source_id());
            observed.sources.slice(node.span()).unwrap()
        };
        assert_eq!(expression_text(binding.initializer()), initializer);
        assert_eq!(expression_text(source.call()), call);
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one actual inner-call handoff");
        };
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one actual source-token transport");
        };
        assert_eq!(handoff.source_loan(), transport.source_loan());
        assert_eq!(handoff.source_loan().call(), source.call());
        assert_eq!(handoff.source_loan().argument(), source.argument());
        assert_eq!(handoff.binding(), binding.binding());
        assert_eq!(transport.binding(), binding.binding());
        assert_eq!(handoff.point(), UnitDropPoint::CallReturn(source.call()));
        assert_eq!(transport.handoff_point(), handoff.point());
        assert_eq!(transport.source_loan().target(), binding.origin());
        assert!(unit.loans().contains(transport.source_loan()));
        let [end] = unit.borrow_results().ends() else {
            panic!("one original result endpoint");
        };
        assert_eq!(end.binding(), binding.binding());
        assert_eq!(transport.point(), end.point());
        assert_ne!(transport.point(), handoff.point());
        assert_eq!(point_text(observed, end.point()), end_text);
        let UnitLoanTarget::Place(root) = binding.origin() else {
            panic!("original owned source");
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(root.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), end.point());
    }
}

#[test]
fn grouped_initializer_transports_the_original_inner_call_token() {
    for initializer in ["(view(owner))", "((view((owner))))"] {
        let call = if initializer.contains("view((owner))") {
            "view((owner))"
        } else {
            "view(owner)"
        };
        let argument = if initializer.contains("view((owner))") {
            "(owner)"
        } else {
            "owner"
        };
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = {initializer}; println(item) }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        super::normal_oracle::assert_normal_handoff(
            &observed,
            declaration(&text, "borrow val", "item"),
            loan_bytes(&text, call, argument, "source: String", "owner"),
            &[],
        );
        assert_transport(&observed, initializer, call, "println(item)");
    }
}

#[test]
fn grouped_named_arguments_keep_source_and_auxiliary_identities() {
    let call = "select(last = last, source = owner, first = first)";
    let initializer = format!("(({call}))");
    let text = format!(
        "fun select(first: String, source: String, last: String): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; val first = \"first\"; val last = \"last\"; borrow val item = {initializer}; println(item) }}"
    );
    let observed = observe(&text);
    assert_units_admitted(&observed);
    // Argument occurrences are the right-hand names, not the named-argument labels.
    let call_span = span(&text, call);
    let argument = |name: &str| {
        let start = span(call, &format!("{name} = {name}")).1 - name.len();
        (call_span.0 + start, call_span.0 + start + name.len())
    };
    let auxiliary = [
        (
            call_span,
            argument("last"),
            span(&text, "last: String"),
            declaration(&text, "val", "last"),
        ),
        (
            call_span,
            argument("first"),
            span(&text, "first: String"),
            declaration(&text, "val", "first"),
        ),
    ];
    super::normal_oracle::assert_normal_handoff(
        &observed,
        declaration(&text, "borrow val", "item"),
        loan_bytes(&text, call, "owner", "source: String", "owner"),
        &auxiliary,
    );
    assert_transport(&observed, &initializer, call, "println(item)");
}

#[test]
fn grouped_abandoned_initializer_observes_only_an_evaluated_source_prefix() {
    for (call, evaluated) in [
        ("select(source = owner, tail = return)", true),
        ("select(tail = return, source = owner)", false),
    ] {
        let text = format!(
            "fun select(source: String, tail: Unit): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = (({call})) }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        let (call, argument, parameter, root) =
            loan_bytes(&text, call, "owner", "source: String", "owner");
        let expected = if evaluated {
            vec![(call, argument, parameter, root, span(&text, "return"))]
        } else {
            vec![]
        };
        super::prefix_oracle::assert_return_prefixes(&observed, &expected);
        for unit in &observed.units {
            assert!(unit.borrow_binding_transports().is_empty());
            assert_eq!(unit.loans().len(), usize::from(evaluated));
            if evaluated {
                assert_eq!(
                    unit.borrow_call_prefix_ends()[0].preceding_call_loan_ends(),
                    0
                );
            }
        }
    }
}

#[test]
fn unused_grouped_binding_ends_after_its_original_declaration() {
    let initializer = "((view(owner)))";
    let call = "view(owner)";
    let text = format!(
        "fun view(source: String): borrow String from source = source\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = {initializer} }}"
    );
    let observed = observe(&text);
    assert_units_admitted(&observed);
    super::normal_oracle::assert_normal_handoff(
        &observed,
        declaration(&text, "borrow val", "item"),
        loan_bytes(&text, call, "owner", "source: String", "owner"),
        &[],
    );
    assert_transport(
        &observed,
        initializer,
        call,
        &format!("borrow val item = {initializer}"),
    );
}

#[test]
fn grouped_binding_keeps_its_source_until_a_later_return() {
    let initializer = "((view(owner)))";
    let call = "view(owner)";
    let text = format!(
        "fun view(source: String): borrow String from source = source\nfun observe(item: String, tail: Unit): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = {initializer}; observe(item, return) }}"
    );
    let observed = observe(&text);
    assert_units_admitted(&observed);
    super::normal_oracle::assert_normal_handoff(
        &observed,
        declaration(&text, "borrow val", "item"),
        loan_bytes(&text, call, "owner", "source: String", "owner"),
        &[],
    );
    assert_transport(&observed, initializer, call, "return");
    for unit in &observed.units {
        assert!(
            unit.borrow_call_prefix_ends().is_empty(),
            "initializer already returned"
        );
    }
}

#[test]
fn grouping_does_not_promote_parent_sources_aliases_or_nested_consumers() {
    for (body, bindings, handoffs) in [
        (
            "borrow val parent = view(owner); borrow val child = ((view(parent))); println(child)",
            2,
            1,
        ),
        ("observe(((view(owner))))", 0, 0),
        ("borrow val item = ((owner)); println(item)", 1, 0),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; {body} }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        super::borrow_handoff::assert_handoff_identity(&observed);
        for unit in &observed.units {
            assert_eq!(unit.borrow_results().bindings().len(), bindings);
            assert_eq!(unit.borrow_call_handoffs().len(), handoffs);
            assert_eq!(unit.borrow_binding_transports().len(), handoffs);
            assert!(unit.borrow_call_prefix_ends().is_empty());
            if handoffs == 1 {
                let parent = unit
                    .borrow_results()
                    .bindings()
                    .iter()
                    .find(|binding| binding.parent().is_none())
                    .unwrap();
                assert_eq!(unit.borrow_call_handoffs()[0].binding(), parent.binding());
                assert_eq!(
                    unit.borrow_binding_transports()[0].binding(),
                    parent.binding()
                );
                assert!(
                    unit.borrow_results()
                        .bindings()
                        .iter()
                        .any(|binding| binding.parent() == Some(parent.binding()))
                );
            }
        }
    }
}

#[test]
fn grouped_cross_source_handoff_preserves_caller_initializer_and_provider_parameter() {
    let mut sources = SourceMap::new();
    let provider = sources
        .add_source(
            "provider.ko",
            "package demo\nfun view(source: String): borrow String from source = source",
        )
        .unwrap();
    let caller_text = "package demo\nfun run(): Unit { val owner = \"kept\"; borrow val item = ((view(owner))); println(item) }";
    let caller = sources.add_source("caller.ko", caller_text).unwrap();
    let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
    let caller_file = parse_file(&sources, &lex(&sources, caller).unwrap()).unwrap();
    assert!(provider_file.diagnostics().is_empty());
    assert!(caller_file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let inputs = [
        SourceUnitInput::new("root", "demo/provider.ko", provider, &provider_file),
        SourceUnitInput::new("root", "demo/caller.ko", caller, &caller_file),
    ];
    for inputs in [inputs, [inputs[1], inputs[0]]] {
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
        assert!(unit.deferred().is_empty());
        let caller_unit = index
            .source_units()
            .iter()
            .find(|source| source.source_id() == caller)
            .unwrap();
        let provider_unit = index
            .source_units()
            .iter()
            .find(|source| source.source_id() == provider)
            .unwrap();
        assert_ne!(caller_unit.id(), provider_unit.id());
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one caller binding");
        };
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one actual handoff");
        };
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one transported source");
        };
        let loan = handoff.source_loan();
        assert_eq!(binding.initializer().source_unit(), caller_unit.id());
        assert_eq!(binding.binding().source_unit(), caller_unit.id());
        assert_eq!(loan.call().source_unit(), caller_unit.id());
        assert_eq!(loan.argument().source_unit(), caller_unit.id());
        assert_eq!(loan.parameter_span().unwrap().source_id(), provider);
        assert_eq!(
            sources.slice(loan.parameter_span().unwrap()),
            Ok("source: String")
        );
        let node = |id: UnitExpressionId| {
            caller_file
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
        };
        assert_eq!(
            sources.slice(node(binding.initializer()).span()),
            Ok("((view(owner)))")
        );
        assert_eq!(sources.slice(node(loan.call()).span()), Ok("view(owner)"));
        assert_eq!(sources.slice(node(loan.argument()).span()), Ok("owner"));
        assert_ne!(binding.initializer(), loan.call());
        assert_eq!(handoff.binding(), binding.binding());
        assert_eq!(transport.binding(), binding.binding());
        assert_eq!(transport.source_loan(), loan);
        assert!(unit.loans().contains(loan));
        assert_eq!(transport.point(), unit.borrow_results().ends()[0].point());
        let UnitDropPoint::AfterStatement(statement) = transport.point() else {
            panic!("actual caller last use");
        };
        assert_eq!(statement.source_unit(), caller_unit.id());
        assert_eq!(
            sources.slice(
                caller_file
                    .ast()
                    .statements()
                    .get(statement.statement())
                    .unwrap()
                    .span()
            ),
            Ok("println(item)")
        );
        let UnitLoanTarget::Place(root) = loan.target() else {
            panic!("original caller root");
        };
        assert_eq!(root.root().source_unit(), caller_unit.id());
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(root.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), transport.point());
        unit.validate().unwrap();
    }
}

#[test]
fn later_errors_and_deferred_work_revoke_grouped_handoff_and_transport() {
    let prefix = "fun view(source: String): borrow String from source = source\nfun run(): Unit { val owner = \"kept\"; borrow val item = ((view(owner))); println(item) }";
    let good = observe(prefix);
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.borrow_call_handoffs().len(), 1);
        assert_eq!(unit.borrow_binding_transports().len(), 1);
    }
    for (suffix, deferred) in [
        ("fun rejected(): Unit { val target = 0; target = 1 }", false),
        (
            "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
            true,
        ),
    ] {
        let rejected = observe(&format!("{prefix}\n{suffix}"));
        for unit in &rejected.units {
            if deferred {
                assert!(unit.diagnostics().is_empty());
                let [fact] = unit.deferred() else {
                    panic!("one real deferred fact");
                };
                assert_eq!(
                    fact.reason(),
                    OwnershipDeferredReason::OwnershipPrimitiveClosureTransport
                );
            } else {
                assert!(unit.deferred().is_empty());
                let [diagnostic] = unit.diagnostics() else {
                    panic!("one real immutable inout error");
                };
                assert_eq!(diagnostic.code().to_string(), "L0134");
                assert_eq!(
                    rejected.sources.slice(diagnostic.primary_span()),
                    Ok("target")
                );
            }
            assert!(unit.borrow_call_handoffs().is_empty());
            assert!(unit.borrow_binding_transports().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}
