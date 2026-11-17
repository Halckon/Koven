//! Provenance is evidence from completed analysis, never an ABI guess.
use super::*;
#[path = "unit_cfg_origin.rs"]
mod unit_cfg_origin;
use lang_frontend::ownership_checking::{
    CallableOrigin, PointerCallableReturnOrigin, UnitCallableOrigin,
};

fn expression(sources: &SourceMap, parsed: &ParsedFile, text: &str) -> ExpressionId {
    let ids = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "unique expression {text}: {ids:?}");
    ids[0]
}

/// Analyze both real callers before querying summaries, so a fact failure hides no fixture error.
fn owned_argument_factory_fixtures() -> [[unit_cfg_origin::Analysis; 2]; 2] {
    let int_case = format!(
        "{FUNCTIONS}\n\
        fun caller(): Unit {{ val result = factory()\ntake(result) }}\n\
        fun consumeInt(own number: Int): Unit {{}}\n\
        fun factory(): (Int)->Int {{ consumeInt(7)\nreturn ({{ index -> index }}) }}"
    );
    let abort_case = format!(
        "{FUNCTIONS}\n\
        fun caller(): Unit {{ val result = factory()\ntake(result) }}\n\
        fun consumeFn(own consumed: (Int)->Int): Unit {{}}\n\
        fun factory(): (Int)->Int {{ consumeFn({{ index -> index + 1 }})\nreturn error(\"stop\") }}"
    );
    let unit_fixtures = [
        unit_cfg_origin::preflight(&int_case),
        unit_cfg_origin::preflight(&abort_case),
    ];
    for (analyses, callable, parameter) in [
        (&unit_fixtures[0], "consumeInt", "number"),
        (&unit_fixtures[1], "consumeFn", "consumed"),
    ] {
        for checked in analyses {
            checked.assert_value_parameter(callable, parameter);
        }
    }
    unit_fixtures
}

#[test]
fn callable_contract_return_statistics_ignore_owned_int_call_arguments() {
    let [int_case, _abort_case] = owned_argument_factory_fixtures();
    for checked in &int_case {
        checked.assert_pointer_lambda("factory", Some("({ index -> index })"), 0);
        for text in ["factory()", "result"] {
            assert_eq!(
                checked.origin(text),
                Some(UnitCallableOrigin::FactoryResult(
                    checked.expression("factory()")
                ))
            );
        }
    }
}

#[test]
fn callable_contract_return_statistics_do_not_turn_owned_fn_argument_into_abort_result() {
    let [_int_case, abort_case] = owned_argument_factory_fixtures();
    for checked in &abort_case {
        assert!(
            checked
                .owned
                .pointer_callable_return(checked.target("factory"))
                .is_none()
        );
        assert!(checked.owned.pointer_callable_returns().is_empty());
        for text in ["factory()", "result"] {
            assert!(checked.origin(text).is_none());
        }
    }
}

#[test]
fn callable_contract_return_statistics_nested_return_in_value_argument_is_a_real_delivery() {
    let text = format!(
        "{FUNCTIONS}\n\
        fun caller(): Unit {{ val result = factory(true)\ntake(result) }}\n\
        fun consume(own item: Int): Unit {{}}\n\
        fun factory(flag: Boolean): (Int)->Int {{\n\
        consume(if(flag) return ({{ index -> index }}) else 1)\nreturn error(\"stop\") }}"
    );
    for checked in unit_cfg_origin::preflight(&text) {
        checked.assert_value_parameter("consume", "item");
        checked.assert_pointer_lambda("factory", Some("({ index -> index })"), 0);
        for text in ["factory(true)", "result"] {
            assert_eq!(
                checked.origin(text),
                Some(UnitCallableOrigin::FactoryResult(
                    checked.expression("factory(true)")
                ))
            );
        }
    }
}

#[test]
fn callable_contract_return_statistics_nested_actual_return_is_not_hidden_by_outer_return() {
    let text = format!(
        "{FUNCTIONS}\n\
        fun caller(): Unit {{ val result = factory(true)\ntake(result) }}\n\
        fun factory(flag: Boolean): (Int)->Int {{\n\
        return if(flag) {{ return ({{ index -> index }}) }} else {{ error(\"stop\") }}\n }}"
    );
    for checked in unit_cfg_origin::preflight(&text) {
        checked.assert_pointer_lambda("factory", Some("({ index -> index })"), 0);
        assert_eq!(checked.owned.pointer_callable_returns().len(), 1);
        for text in ["factory(true)", "result"] {
            assert_eq!(
                checked.origin(text),
                Some(UnitCallableOrigin::FactoryResult(
                    checked.expression("factory(true)")
                ))
            );
        }
    }
}

#[test]
fn callable_contract_lambda_alias_group_and_recursive_parameter_forwarding() {
    let text = format!(
        "{FUNCTIONS}\n\
        fun relay(forwarded: (Int) -> Int): Unit {{ take((forwarded))\nrelay(forwarded) }}\n\
        fun use(): Unit {{ val first: (Int) -> Int = {{ index -> index }}\n\
        val alias = (first)\ntake((alias)) }}"
    );
    for checked in unit_cfg_origin::preflight(&text) {
        checked.assert_alias_and_parameter_origins();
    }
}

#[test]
fn callable_contract_annotated_symbol_does_not_select_deferred_function_operand() {
    let text = format!(
        "{FUNCTIONS}\nfun use(): Unit {{\n\
        val annotated: (Int) -> Int = identity\nval alias = (annotated)\ntake(alias) }}"
    );
    let (sources, parsed, names, typed, owned) = preflight(&text);
    assert!(matches!(
        typed
            .symbol_type(symbol(&names, "annotated"))
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Function { .. })
    ));
    assert!(matches!(
        typed
            .expression_type(expression(&sources, &parsed, "identity"))
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Deferred(_))
    ));
    for value in ["identity", "annotated", "(annotated)", "alias"] {
        assert!(
            owned
                .callable_origin(expression(&sources, &parsed, value))
                .is_none(),
            "{value} cannot invent missing selection"
        );
    }
}

#[test]
fn callable_contract_caller_first_factory_freezes_source_identity_and_aliases() {
    let text = format!(
        "{FUNCTIONS}\n\
        fun caller(): Unit {{ val result = factory()\nval alias = (result)\ntake(alias) }}\n\
        fun factory(): (Int) -> Int {{ println(\"factory\")\nreturn ({{ index -> index }}) }}\n\
        fun namedFactory(): (Int) -> Int = (identity)"
    );
    let (sources, parsed, names, typed, owned) = preflight(&text);
    let target = symbol(&names, "factory");
    let summary = owned
        .pointer_callable_return(target)
        .expect("one normal empty-environment return");
    assert_eq!(owned.pointer_callable_returns().len(), 1);
    assert_eq!(summary.target(), target);
    assert_eq!(
        summary.origin(),
        PointerCallableReturnOrigin::Lambda(lambdas(&parsed)[0])
    );
    assert_eq!(
        summary.return_value(),
        expression(&sources, &parsed, "({ index -> index })")
    );
    assert_eq!(
        summary.span(),
        parsed
            .ast()
            .expressions()
            .get(summary.return_value())
            .unwrap()
            .span()
    );
    assert!(matches!(
        typed.types().get(summary.function_type()),
        Some(TypeKind::Function { .. })
    ));
    assert!(
        owned
            .pointer_callable_return(symbol(&names, "namedFactory"))
            .is_none(),
        "Deferred operand cannot define pointer ABI"
    );
    let call = expression(&sources, &parsed, "factory()");
    for value in ["factory()", "result", "(result)", "alias"] {
        assert_eq!(
            owned
                .callable_origin(expression(&sources, &parsed, value))
                .unwrap()
                .origin(),
            CallableOrigin::FactoryResult(call)
        );
    }
}

#[test]
fn callable_contract_legal_multiple_and_capturing_returns_publish_no_pointer_summary() {
    let text = format!(
        "{FUNCTIONS}\n\
        fun multiple(flag: Boolean): (Int) -> Int {{ if(flag) {{ return ({{ index -> index }}) }}\nreturn ({{ index -> index + 1 }}) }}\n\
        fun captured(own label: String): (Int) -> Boolean = move {{ index -> label == \"kept\" && index == 0 }}"
    );
    for checked in unit_cfg_origin::preflight(&text) {
        for name in ["multiple", "captured"] {
            assert!(
                checked
                    .owned
                    .pointer_callable_return(checked.target(name))
                    .is_none()
            );
        }
        assert!(checked.owned.pointer_callable_returns().is_empty());
    }
}

#[test]
fn callable_contract_loop_fixed_point_preserves_stable_and_overwritten_origins() {
    let stable = format!(
        "{FUNCTIONS}\nfun use(flag: Boolean): Unit {{\n\
        var callback: (Int) -> Int = {{ index -> index }}\nwhile(flag) {{ take((callback)) }} }}"
    );
    let overwrite = format!(
        "{FUNCTIONS}\nfun use(flag: Boolean): Unit {{\n\
        var callback: (Int) -> Int = {{ index -> index }}\nwhile(flag) {{\n\
        val next: (Int)->Int = {{ index -> index + 1 }}\ncallback = next\nval alias = (callback)\ntake((alias))\n\
        val again: (Int)->Int = {{ index -> index + 2 }}\ncallback = again\n }} }}"
    );
    let stable_analysis = unit_cfg_origin::preflight(&stable);
    let overwrite_analysis = unit_cfg_origin::preflight(&overwrite);
    for checked in stable_analysis {
        assert_eq!(checked.origin("callback").unwrap(), checked.lambda(0));
    }
    for checked in overwrite_analysis {
        assert_eq!(
            checked.origin("alias").unwrap(),
            checked.lambda(1),
            "overwrite before use cuts unstable header dependency"
        );
    }
}

#[test]
fn callable_contract_loop_backedge_and_repeated_condition_revoke_unstable_origins() {
    let conflict = format!(
        "{FUNCTIONS}\nfun use(flag: Boolean): Unit {{\n\
        var callback: (Int) -> Int = {{ index -> index }}\nwhile(flag) {{\n\
        val alias = (callback)\ntake((alias))\nval next: (Int)->Int = {{ index -> index + 1 }}\ncallback = next\n }} }}"
    );
    for checked in unit_cfg_origin::preflight(&conflict) {
        assert!(checked.origin("alias").is_none());
    }
    let condition = format!(
        "{FUNCTIONS}\nfun decide(f: (Int) -> Int): Boolean = false\n\
        fun use(): Unit {{ var callback: (Int) -> Int = {{ index -> index }}\n\
        while(decide((callback))) {{ val next: (Int)->Int = {{ index -> index + 1 }}\ncallback = next }} }}"
    );
    for checked in unit_cfg_origin::preflight(&condition) {
        assert!(checked.origin("(callback)").is_none());
    }
}

#[test]
fn callable_contract_p2_and_p3_errors_clear_both_public_tables() {
    let (_, _, _, _, owned) = analyzed_with(
        "fun bad(label: String): (Int)->Boolean = move { index -> label == \"borrowed\" && index == 0 }",
        standard_environments(),
    );
    assert_eq!(codes(&owned), ["L0138"]);
    assert!(owned.callable_origins().is_empty());
    assert!(owned.pointer_callable_returns().is_empty());
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "typed-error.ko",
            "fun good(): (Int)->Int = { index -> index }\nfun invalid(): (Int)->Int = 1",
        )
        .unwrap();
    let parsed = parse_file_twice(&sources, source, "typed error with valid candidate");
    let (environment, type_environment) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &type_environment).unwrap();
    assert!(!typed.diagnostics().is_empty());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(
        owned.callable_origins().is_empty(),
        "P2 recovery cannot publish another function's valid provenance"
    );
    assert!(owned.pointer_callable_returns().is_empty());
}

#[test]
fn callable_contract_cfg_join_requires_the_same_origin_on_every_normal_path() {
    let text = format!(
        "{FUNCTIONS}\nfun use(flag: Boolean): Unit {{\n\
        val original: (Int)->Int = {{ index -> index }}\n\
        val same: (Int)->Int = if(flag) {{ original }} else {{ original }}\ntake(same)\n\
        val first: (Int)->Int = {{ index -> index + 1 }}\nval second: (Int)->Int = {{ index -> index + 2 }}\n\
        val different: (Int)->Int = if(flag) {{ first }} else {{ second }}\ntake(different) }}"
    );
    for checked in unit_cfg_origin::preflight(&text) {
        assert_eq!(checked.origin("same").unwrap(), checked.lambda(0));
        assert!(checked.origin("different").is_none());
    }
}

#[test]
fn callable_contract_break_is_an_exit_but_continue_is_a_repeating_edge() {
    let cases = [("break", true), ("continue", false)].map(|(jump, known)| {
        let text = format!(
            "{FUNCTIONS}\nfun use(flag: Boolean): Unit {{\n\
            var callback: (Int)->Int = {{ index -> index }}\nwhile(flag) {{\n\
            take((callback))\nval next: (Int)->Int = {{ index -> index + 1 }}\ncallback = next\n{jump}\n }} }}"
        );
        (jump, known, unit_cfg_origin::preflight(&text))
    });
    for (jump, known, analyses) in cases {
        for checked in analyses {
            assert_eq!(
                checked.origin("(callback)").is_some(),
                known,
                "{jump} must connect to the right loop boundary"
            );
        }
    }
}

#[test]
fn callable_contract_for_source_is_not_a_repeated_header_use() {
    let text = format!(
        "{FUNCTIONS}\n\
        fun source(f: (Int)->Int): List<Int> = List<Int>(1, {{ index -> index }})\n\
        fun use(): Unit {{ var callback: (Int)->Int = {{ index -> index + 1 }}\n\
        for (_ in source((callback))) {{ val again: (Int)->Int = {{ index -> index + 2 }}\ncallback = again }} }}"
    );
    for checked in unit_cfg_origin::preflight(&text) {
        assert_eq!(checked.origin("(callback)").unwrap(), checked.lambda(1));
    }
}

#[test]
fn callable_contract_abort_return_and_lambda_inner_return_are_not_outer_deliveries() {
    let text = "fun factory(flag: Boolean): (Int)->Int { if(flag) { return error(\"stop\") }\nreturn ({ index -> return index }) }";
    for checked in unit_cfg_origin::preflight(text) {
        checked.assert_pointer_lambda("factory", None, 0);
    }
}
