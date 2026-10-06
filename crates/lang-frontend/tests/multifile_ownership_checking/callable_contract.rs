//! Source-qualified provenance must agree with actual selected typed facts.
use super::*;
use lang_frontend::{
    name_resolution::UnitReferenceTarget,
    ownership_checking::{UnitCallableOrigin, UnitPointerCallableReturnOrigin},
    parser::Expression,
    type_checking::UnitCallableTarget,
};

fn expression(
    sources: &SourceMap,
    file: &ParsedFile,
    unit: SourceUnitId,
    text: &str,
) -> UnitExpressionId {
    let ids = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (sources.slice(node.span()).unwrap() == text).then_some(UnitExpressionId::new(unit, id))
        })
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "unique expression {text}: {ids:?}");
    ids[0]
}

fn target(fixture: &Fixture, name: &str) -> UnitCallableTarget {
    let declarations = fixture
        .names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| declaration.name() == name)
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 1, "unique source declaration {name}");
    UnitCallableTarget::Declaration(declarations[0].id())
}

fn local_symbol(fixture: &Fixture, unit: SourceUnitId, name: &str) -> UnitSymbolId {
    let resolution = fixture.names.names().source_units()[unit.index()].resolution();
    let local = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .unwrap()
        .id();
    fixture
        .typed
        .types()
        .body_symbol_types()
        .keys()
        .copied()
        .find(|symbol| symbol.source_unit() == unit && symbol.symbol() == local)
        .expect("source-qualified typed local symbol")
}

/// p sorts before q: both factories must be finalized after their real callers were analyzed.
fn owned_argument_factory_fixtures() -> [Fixture; 2] {
    let caller = "package p\nimport q.factory\nimport q.take\n\
        fun caller(): Unit { val result = factory()\ntake(result) }";
    let int_factory = "package q\nfun take(callback: (Int)->Int): Unit {}\n\
        fun consumeInt(own number: Int): Unit {}\n\
        fun factory(): (Int)->Int { consumeInt(7)\nreturn ({ index -> index }) }";
    let abort_factory = "package q\nfun take(callback: (Int)->Int): Unit {}\n\
        fun consumeFn(own consumed: (Int)->Int): Unit {}\n\
        fun factory(): (Int)->Int { consumeFn({ index -> index + 1 })\nreturn error(\"stop\") }";
    // Complete parser/name/type/ownership gates for both fixtures before any fact assertion.
    let fixtures = [
        preflight(caller, int_factory),
        preflight(caller, abort_factory),
    ];
    for (fixture, parameter_name) in fixtures.iter().zip(["number", "consumed"]) {
        let parameter = fixture
            .typed
            .types()
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.callable())
            .flat_map(|callable| callable.parameters())
            .find(|parameter| parameter.name() == Some(parameter_name))
            .expect("actual named consume parameter");
        assert_eq!(parameter.mode(), ParameterMode::Value);
    }
    fixtures
}

#[test]
fn callable_contract_return_statistics_ignore_owned_int_call_arguments() {
    let [fixture, _abort_fixture] = owned_argument_factory_fixtures();
    let factory_unit = source_unit(&fixture.names, fixture.consumer_source);
    let caller_unit = source_unit(&fixture.names, fixture.provider_source);
    let summary = fixture
        .owned
        .pointer_callable_return(target(&fixture, "factory"))
        .expect("consuming an Int is not a second normal factory return");
    assert_eq!(
        summary.return_value(),
        expression(
            &fixture.sources,
            &fixture.consumer,
            factory_unit,
            "({ index -> index })"
        )
    );
    assert_eq!(
        summary.origin(),
        UnitPointerCallableReturnOrigin::Lambda(expression(
            &fixture.sources,
            &fixture.consumer,
            factory_unit,
            "{ index -> index }"
        ))
    );
    let call = expression(
        &fixture.sources,
        &fixture.provider,
        caller_unit,
        "factory()",
    );
    for text in ["factory()", "result"] {
        assert_eq!(
            fixture
                .owned
                .callable_origin(expression(
                    &fixture.sources,
                    &fixture.provider,
                    caller_unit,
                    text
                ))
                .unwrap()
                .origin(),
            UnitCallableOrigin::FactoryResult(call)
        );
    }
}

#[test]
fn callable_contract_return_statistics_do_not_turn_owned_fn_argument_into_abort_result() {
    let [_int_fixture, fixture] = owned_argument_factory_fixtures();
    let caller_unit = source_unit(&fixture.names, fixture.provider_source);
    assert!(
        fixture
            .owned
            .pointer_callable_return(target(&fixture, "factory"))
            .is_none(),
        "the consumed lambda is a call argument; this factory has no normal return"
    );
    assert!(fixture.owned.pointer_callable_returns().is_empty());
    for text in ["factory()", "result"] {
        assert!(
            fixture
                .owned
                .callable_origin(expression(
                    &fixture.sources,
                    &fixture.provider,
                    caller_unit,
                    text
                ))
                .is_none(),
            "an abort-only callee cannot justify FactoryResult for {text}"
        );
    }
}

#[test]
fn callable_contract_return_statistics_nested_return_in_value_argument_is_a_real_delivery() {
    let fixture = preflight(
        "package p\nimport q.factory\nimport q.take\n\
        fun caller(): Unit { val result = factory(true)\ntake(result) }",
        "package q\nfun take(callback: (Int)->Int): Unit {}\n\
        fun consume(own item: Int): Unit {}\n\
        fun factory(flag: Boolean): (Int)->Int {\n\
        consume(if(flag) return ({ index -> index }) else 1)\nreturn error(\"stop\") }",
    );
    let parameter = fixture
        .typed
        .types()
        .signatures()
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.callable())
        .flat_map(|callable| callable.parameters())
        .find(|parameter| parameter.name() == Some("item"))
        .unwrap();
    assert_eq!(parameter.mode(), ParameterMode::Value);
    let factory_unit = source_unit(&fixture.names, fixture.consumer_source);
    let caller_unit = source_unit(&fixture.names, fixture.provider_source);
    let summary = fixture
        .owned
        .pointer_callable_return(target(&fixture, "factory"))
        .expect("an explicit return nested inside a Value argument exits the actual factory");
    assert_eq!(
        summary.return_value(),
        expression(
            &fixture.sources,
            &fixture.consumer,
            factory_unit,
            "({ index -> index })"
        )
    );
    assert_eq!(
        summary.origin(),
        UnitPointerCallableReturnOrigin::Lambda(expression(
            &fixture.sources,
            &fixture.consumer,
            factory_unit,
            "{ index -> index }"
        ))
    );
    let call = expression(
        &fixture.sources,
        &fixture.provider,
        caller_unit,
        "factory(true)",
    );
    for text in ["factory(true)", "result"] {
        assert_eq!(
            fixture
                .owned
                .callable_origin(expression(
                    &fixture.sources,
                    &fixture.provider,
                    caller_unit,
                    text
                ))
                .unwrap()
                .origin(),
            UnitCallableOrigin::FactoryResult(call)
        );
    }
}

#[test]
fn callable_contract_return_statistics_nested_actual_return_is_not_hidden_by_outer_return() {
    let fixture = preflight(
        "package p\nimport q.factory\nimport q.take\n\
        fun caller(): Unit { val result = factory(true)\ntake(result) }",
        "package q\nfun take(callback: (Int)->Int): Unit {}\n\
        fun factory(flag: Boolean): (Int)->Int {\n\
        return if(flag) { return ({ index -> index }) } else { error(\"stop\") }\n }",
    );
    let factory_unit = source_unit(&fixture.names, fixture.consumer_source);
    let caller_unit = source_unit(&fixture.names, fixture.provider_source);
    let summary = fixture
        .owned
        .pointer_callable_return(target(&fixture, "factory"))
        .expect("the inner actual return completes even though the enclosing operand never does");
    assert_eq!(
        summary.return_value(),
        expression(
            &fixture.sources,
            &fixture.consumer,
            factory_unit,
            "({ index -> index })"
        )
    );
    assert_eq!(
        summary.origin(),
        UnitPointerCallableReturnOrigin::Lambda(expression(
            &fixture.sources,
            &fixture.consumer,
            factory_unit,
            "{ index -> index }"
        ))
    );
    assert_eq!(fixture.owned.pointer_callable_returns().len(), 1);
    let call = expression(
        &fixture.sources,
        &fixture.provider,
        caller_unit,
        "factory(true)",
    );
    for text in ["factory(true)", "result"] {
        assert_eq!(
            fixture
                .owned
                .callable_origin(expression(
                    &fixture.sources,
                    &fixture.provider,
                    caller_unit,
                    text
                ))
                .unwrap()
                .origin(),
            UnitCallableOrigin::FactoryResult(call)
        );
    }
}

#[test]
fn callable_contract_unique_source_function_requires_both_declaration_and_function_type() {
    let fixture = preflight(
        PROVIDER,
        "package q\nimport p.identity\nimport p.take\n\
        fun use(): Unit { val named = identity\nval alias = (named)\ntake((alias)) }",
    );
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    let known = target(&fixture, "identity");
    let use_id = expression(&fixture.sources, &fixture.consumer, unit, "identity");
    assert!(matches!(
        fixture
            .typed
            .types()
            .expression_type(use_id)
            .and_then(|ty| fixture.typed.types().types().get(ty)),
        Some(UnitTypeKind::Function { .. })
    ));
    let node = fixture
        .consumer
        .ast()
        .expressions()
        .get(use_id.expression())
        .unwrap();
    assert!(matches!(node.payload(), Expression::Name));
    assert!(fixture.names.names().references().iter().any(|reference| reference.span() == node.span() && matches!(reference.target(), UnitReferenceTarget::Declaration(declaration) if known == UnitCallableTarget::Declaration(*declaration))));
    for name in ["named", "alias"] {
        let symbol = local_symbol(&fixture, unit, name);
        assert!(matches!(
            fixture
                .typed
                .types()
                .symbol_type(symbol)
                .and_then(|ty| fixture.typed.types().types().get(ty)),
            Some(UnitTypeKind::Function { .. })
        ));
    }
    for text in ["identity", "named", "(named)", "alias", "(alias)"] {
        assert_eq!(
            fixture
                .owned
                .callable_origin(expression(&fixture.sources, &fixture.consumer, unit, text))
                .unwrap()
                .origin(),
            UnitCallableOrigin::KnownFunction(known)
        );
    }
}

#[test]
fn callable_contract_lambda_alias_group_and_recursive_parameter_forwarding() {
    let consumer = "package q\nimport p.take\n\
        fun relay(forwarded: (Int)->Int): Unit { take((forwarded))\nrelay(forwarded) }\n\
        fun use(): Unit { val first: (Int)->Int = { index -> index }\nval alias = (first)\ntake((alias)) }";
    let fixture = preflight(PROVIDER, consumer);
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    let lambda = expression(
        &fixture.sources,
        &fixture.consumer,
        unit,
        "{ index -> index }",
    );
    for text in ["first", "(first)", "alias", "(alias)"] {
        let use_id = expression(&fixture.sources, &fixture.consumer, unit, text);
        let fact = fixture.owned.callable_origin(use_id).unwrap();
        assert_eq!(fact.expression(), use_id);
        assert_eq!(fact.origin(), UnitCallableOrigin::Lambda(lambda));
        assert_eq!(
            fact.span(),
            fixture
                .consumer
                .ast()
                .expressions()
                .get(use_id.expression())
                .unwrap()
                .span()
        );
    }
    let forwarded = symbol_named(&fixture.owned, &fixture.names, unit, "forwarded");
    for (id, node) in fixture.consumer.ast().expressions().iter() {
        if fixture.sources.slice(node.span()).unwrap() == "forwarded" {
            assert_eq!(
                fixture
                    .owned
                    .callable_origin(UnitExpressionId::new(unit, id))
                    .unwrap()
                    .origin(),
                UnitCallableOrigin::Parameter(forwarded)
            );
        }
    }
    for fact in fixture.owned.callable_origins() {
        assert!(matches!(
            fixture
                .typed
                .types()
                .expression_type(fact.expression())
                .and_then(|ty| fixture.typed.types().types().get(ty)),
            Some(UnitTypeKind::Function { .. })
        ));
    }
}

#[test]
fn callable_contract_caller_first_factory_and_named_return_freeze_source_identity() {
    // p sorts first and contains the caller; q contains both factories.
    let fixture = preflight(
        "package p\nimport q.factory\nimport q.namedFactory\nimport q.take\n\
        fun caller(): Unit { val result = factory()\nval alias = (result)\ntake(alias)\ntake(namedFactory()) }",
        "package q\nfun identity(index: Int): Int = index\nfun take(f: (Int)->Int): Unit {}\n\
        fun factory(): (Int)->Int { println(\"factory\")\nreturn ({ index -> index }) }\n\
        fun namedFactory(): (Int)->Int = (identity)",
    );
    let caller_unit = source_unit(&fixture.names, fixture.provider_source);
    let factory_unit = source_unit(&fixture.names, fixture.consumer_source);
    let summary = fixture
        .owned
        .pointer_callable_return(target(&fixture, "factory"))
        .unwrap();
    let lambda = expression(
        &fixture.sources,
        &fixture.consumer,
        factory_unit,
        "{ index -> index }",
    );
    assert_eq!(
        summary.origin(),
        UnitPointerCallableReturnOrigin::Lambda(lambda)
    );
    assert_eq!(summary.target(), target(&fixture, "factory"));
    assert_eq!(
        summary.return_value(),
        expression(
            &fixture.sources,
            &fixture.consumer,
            factory_unit,
            "({ index -> index })"
        )
    );
    assert!(matches!(
        fixture.typed.types().types().get(summary.function_type()),
        Some(UnitTypeKind::Function { .. })
    ));
    let named = fixture
        .owned
        .pointer_callable_return(target(&fixture, "namedFactory"))
        .unwrap();
    assert_eq!(
        named.origin(),
        UnitPointerCallableReturnOrigin::KnownFunction(target(&fixture, "identity"))
    );
    assert_eq!(fixture.owned.pointer_callable_returns().len(), 2);
    let call = expression(
        &fixture.sources,
        &fixture.provider,
        caller_unit,
        "factory()",
    );
    for text in ["factory()", "result", "(result)", "alias"] {
        assert_eq!(
            fixture
                .owned
                .callable_origin(expression(
                    &fixture.sources,
                    &fixture.provider,
                    caller_unit,
                    text
                ))
                .unwrap()
                .origin(),
            UnitCallableOrigin::FactoryResult(call)
        );
    }
}

#[test]
fn callable_contract_function_type_alone_does_not_make_generic_reference_known() {
    let fixture = preflight(
        "package p\nfun <T> identity(index: Int): Int = index\nfun take(f: (Int)->Int): Unit {}",
        "package q\nimport p.identity\nimport p.take\nfun use(): Unit { val named = identity }",
    );
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    assert!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "identity"
            ))
            .is_none(),
        "uninstantiated generic reference cannot pick a pointer instance"
    );
}

#[test]
fn callable_contract_legal_multiple_and_capturing_returns_publish_no_summary() {
    let provider = format!(
        "{PROVIDER}\n\
        fun multiple(flag: Boolean): (Int)->Int {{ if(flag) {{ return identity }}\nreturn other }}\n\
        fun captured(own label: String): (Int)->Boolean = move {{ index -> label == \"kept\" && index == 0 }}"
    );
    let fixture = preflight(&provider, "package q\nfun unused(): Unit {}");
    assert!(
        fixture
            .owned
            .pointer_callable_return(target(&fixture, "multiple"))
            .is_none()
    );
    assert!(
        fixture
            .owned
            .pointer_callable_return(target(&fixture, "captured"))
            .is_none()
    );
    assert!(fixture.owned.pointer_callable_returns().is_empty());
}

#[test]
fn callable_contract_loop_fixed_point_preserves_stable_and_overwritten_origins() {
    let stable = preflight(
        PROVIDER,
        "package q\nimport p.take\nfun use(flag: Boolean): Unit {\n\
        var callback: (Int)->Int = { index -> index }\nwhile(flag) { take((callback)) } }",
    );
    let overwritten = preflight(
        PROVIDER,
        "package q\nimport p.take\nfun use(flag: Boolean): Unit {\n\
        var callback: (Int)->Int = { index -> index }\nwhile(flag) {\n\
        callback = { index -> index + 1 }\nval alias = (callback)\ntake((alias))\n\
        callback = { index -> index + 2 }\n } }",
    );
    let fixture = stable;
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    assert_eq!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "callback"
            ))
            .unwrap()
            .origin(),
        UnitCallableOrigin::Lambda(expression(
            &fixture.sources,
            &fixture.consumer,
            unit,
            "{ index -> index }"
        ))
    );
    let fixture = overwritten;
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    assert_eq!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "alias"
            ))
            .unwrap()
            .origin(),
        UnitCallableOrigin::Lambda(expression(
            &fixture.sources,
            &fixture.consumer,
            unit,
            "{ index -> index + 1 }"
        ))
    );
}

#[test]
fn callable_contract_loop_backedge_and_repeated_condition_revoke_unstable_origins() {
    let fixture = preflight(
        PROVIDER,
        "package q\nimport p.take\nfun use(flag: Boolean): Unit {\n\
        var callback: (Int)->Int = { index -> index }\nwhile(flag) {\n\
        val alias = (callback)\ntake((alias))\ncallback = { index -> index + 1 }\n } }",
    );
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    assert!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "alias"
            ))
            .is_none()
    );
    let fixture = preflight(
        PROVIDER,
        "package q\nfun decide(f: (Int)->Int): Boolean = false\n\
        fun use(): Unit { var callback: (Int)->Int = { index -> index }\n\
        while(decide((callback))) { callback = { index -> index + 1 } } }",
    );
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    assert!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "(callback)"
            ))
            .is_none()
    );
}

#[test]
fn callable_contract_p3_error_clears_facts_and_existing_witness_rejects_mixed_analysis() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "bad.ko",
        "fun good(): (Int)->Int = { index -> index }\nfun invalid(label: String): (Int)->Boolean = move { index -> label == \"borrowed\" && index == 0 }",
    );
    let inputs = [SourceUnitInput::new("root", "bad.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &environment);
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed).unwrap();
    assert_eq!(diagnostic_codes(&owned), ["L0138"]);
    assert!(owned.callable_origins().is_empty());
    assert!(owned.pointer_callable_returns().is_empty());

    let fixture = preflight(
        PROVIDER,
        "package q\nfun factory(): (Int)->Int = { index -> index }",
    );
    assert!(!fixture.owned.pointer_callable_returns().is_empty());
    let inputs = [
        SourceUnitInput::new(
            "root",
            "p/api.ko",
            fixture.provider_source,
            &fixture.provider,
        ),
        SourceUnitInput::new(
            "root",
            "q/use.ko",
            fixture.consumer_source,
            &fixture.consumer,
        ),
    ];
    let fresh = validated_types(
        &fixture.sources,
        &inputs,
        &fixture.names,
        &fixture.environment,
    );
    let owned = fixture.owned.validate().unwrap();
    assert!(matches!(
        owned_compilation_unit_view(
            &fixture.sources,
            &inputs,
            &fixture.names,
            &fixture.environment,
            &fresh,
            &owned
        ),
        Err(OwnedCompilationUnitViewError::MismatchedAnalysis)
    ));
}

#[test]
fn callable_contract_cfg_join_requires_the_same_origin_on_every_normal_path() {
    let consumer = format!(
        "{IMPORTS}\nfun use(flag: Boolean): Unit {{\n\
        val same: (Int)->Int = if(flag) {{ identity }} else {{ identity }}\ntake(same)\n\
        val different: (Int)->Int = if(flag) {{ identity }} else {{ other }}\ntake(different) }}"
    );
    let fixture = preflight(PROVIDER, &consumer);
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    assert_eq!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "same"
            ))
            .unwrap()
            .origin(),
        UnitCallableOrigin::KnownFunction(target(&fixture, "identity"))
    );
    assert!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "different"
            ))
            .is_none()
    );
}

#[test]
fn callable_contract_break_is_an_exit_but_continue_is_a_repeating_edge() {
    let cases = [("break", true), ("continue", false)].map(|(jump, known)| {
        let consumer = format!(
            "package q\nimport p.take\nfun use(flag: Boolean): Unit {{\n\
            var callback: (Int)->Int = {{ index -> index }}\nwhile(flag) {{\n\
            take((callback))\ncallback = {{ index -> index + 1 }}\n{jump}\n }} }}"
        );
        (jump, known, preflight(PROVIDER, &consumer))
    });
    for (jump, known, fixture) in cases {
        let unit = source_unit(&fixture.names, fixture.consumer_source);
        assert_eq!(
            fixture
                .owned
                .callable_origin(expression(
                    &fixture.sources,
                    &fixture.consumer,
                    unit,
                    "(callback)"
                ))
                .is_some(),
            known,
            "{jump} must connect to the right loop boundary"
        );
    }
}

#[test]
fn callable_contract_for_source_is_not_a_repeated_header_use() {
    let consumer = "package q\nfun source(f: (Int)->Int): List<Int> = List<Int>(1, { index -> index })\n\
        fun use(): Unit { var callback: (Int)->Int = { index -> index + 1 }\n\
        for (_ in source((callback))) { callback = { index -> index + 2 } } }";
    let fixture = preflight(PROVIDER, consumer);
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    assert_eq!(
        fixture
            .owned
            .callable_origin(expression(
                &fixture.sources,
                &fixture.consumer,
                unit,
                "(callback)"
            ))
            .unwrap()
            .origin(),
        UnitCallableOrigin::Lambda(expression(
            &fixture.sources,
            &fixture.consumer,
            unit,
            "{ index -> index + 1 }"
        ))
    );
}

#[test]
fn callable_contract_abort_return_and_lambda_inner_return_are_not_outer_deliveries() {
    let fixture = preflight(
        PROVIDER,
        "package q\nfun factory(flag: Boolean): (Int)->Int { if(flag) { return error(\"stop\") }\nreturn ({ index -> return index }) }",
    );
    let unit = source_unit(&fixture.names, fixture.consumer_source);
    let summary = fixture
        .owned
        .pointer_callable_return(target(&fixture, "factory"))
        .unwrap();
    assert_eq!(
        summary.origin(),
        UnitPointerCallableReturnOrigin::Lambda(expression(
            &fixture.sources,
            &fixture.consumer,
            unit,
            "{ index -> return index }"
        ))
    );
}
