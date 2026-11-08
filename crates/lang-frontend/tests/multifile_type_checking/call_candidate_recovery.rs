use super::*;

#[test]
fn selected_calls_ignore_resolved_container_callee_placeholders() {
    let direct = "consume(MutableList<Int>())";
    let overloaded = "choose({ item -> consume(MutableList<Int>()) })";
    let text = format!(
        "fun consume(items: MutableList<Int>): Int = 1\n\
         fun choose(callback: (borrow Int) -> Int): Int = 1\n\
         fun choose(callback: (borrow String) -> String): String = \"other\"\n\
         fun entry(): Unit {{ val first = {direct}; val second = {overloaded} }}"
    );
    check_storage_instances(&text, |sources, file, unit, typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        for call in [direct, overloaded] {
            let id = UnitExpressionId::new(unit, expression_with_text(sources, file, call));
            let descriptor = typed
                .calls()
                .iter()
                .find(|fact| fact.expression() == id)
                .unwrap();
            assert_eq!(
                typed.types().get(descriptor.return_type()),
                Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            );
        }
        assert_eq!(typed.container_constructions().len(), 2);
        typed.clone().validate().unwrap();
    });
}

#[test]
fn rejected_selected_call_does_not_publish_diagnostic_only_context() {
    for (parameter, argument, primary) in [
        ("List<T>", "(\"wrong\")", "\"wrong\""),
        ("List<T>", "input", "input"),
        ("List<T>", "text()", "text()"),
        ("List<T>", "!true", "!true"),
        ("List<T>", "1 == 2", "1 == 2"),
        ("List<T>", "null", "null"),
        ("(List<T>) -> Int", "{ a, b -> 0 }", "->"),
    ] {
        let text = format!(
            "class Holder<T>(val payload: Int)\n\
             class Factory<T> {{ fun choose(value: {parameter}): Holder<T> = error(\"never\") }}\n\
             fun text(): String = \"wrong\"\n\
             fun entry(input: String): Unit {{ val rejected = Factory<Long>().choose({argument}) }}"
        );
        check_storage_instances(&text, |sources, _, _, typed| {
            let [error] = typed.diagnostics() else {
                panic!("{text}: {:?}", typed.diagnostics());
            };
            assert_eq!(error.code().to_string(), "L0084");
            assert_eq!(sources.slice(error.primary_span()), Ok(primary));
            let long = typed.types().builtin(BuiltinType::Long).unwrap();
            assert!(
                typed
                    .types()
                    .find(&UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::List,
                        arguments: vec![long],
                    })
                    .is_none(),
                "a diagnostic-only expectation must not enter the canonical table: {text}"
            );
        });
    }
}

#[test]
fn rejected_selected_call_keeps_errors_from_generic_inference_precheck() {
    let call = r#"choose(input, "${if (true) 7 else false}")"#;
    let text = format!(
        "class Holder<T>(val payload: Int)\n\
         fun <T> choose(tag: T, value: String): Holder<T> = error(\"never\")\n\
         fun entry(input: Long): Unit {{ val rejected = {call} }}"
    );
    check_storage_instances(&text, |sources, file, unit, typed| {
        let [error] = typed.diagnostics() else {
            panic!("{text}: {:?}", typed.diagnostics());
        };
        assert_eq!(error.code().to_string(), "L0089");
        let rejected = UnitExpressionId::new(unit, expression_with_text(sources, file, call));
        assert!(
            typed
                .calls()
                .iter()
                .all(|fact| fact.expression() != rejected)
        );
    });
}

#[test]
fn erroneous_argument_does_not_add_an_overload_failure() {
    let call = r#"choose({ value -> 0 }, "${if (true) 7 else false}")"#;
    let text = format!(
        "fun choose(view: (borrow Int) -> Int, value: String): Unit {{}}\n\
         fun choose(view: (borrow String) -> String, value: String): Unit {{}}\n\
         fun entry(): Unit {{ {call} }}"
    );
    check_storage_instances(&text, |sources, file, unit, typed| {
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            ["L0089"]
        );
        let rejected = UnitExpressionId::new(unit, expression_with_text(sources, file, call));
        assert!(
            typed
                .calls()
                .iter()
                .all(|fact| fact.expression() != rejected)
        );
    });
}

#[test]
fn accepted_selected_call_remaps_context_after_an_independent_argument() {
    let call = "Factory<Long>().choose(make(true), { values -> values.size })";
    let text = format!(
        "class Holder<T>(val payload: T)\n\
         class Factory<T> {{ fun choose(value: String, view: (List<T>) -> Int): Holder<T> = error(\"never\") }}\n\
         fun <T> make(tag: T): String = \"ok\"\n\
         fun entry(): Unit {{ val accepted = {call} }}"
    );
    check_storage_instances(&text, |sources, file, unit, typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let call_fact = |text| {
            let expression = UnitExpressionId::new(unit, expression_with_text(sources, file, text));
            typed
                .calls()
                .iter()
                .find(|fact| fact.expression() == expression)
                .unwrap()
        };
        let types = typed.types();
        let long = types.builtin(BuiltinType::Long).unwrap();
        let string = types.builtin(BuiltinType::String).unwrap();
        assert_eq!(call_fact("make(true)").return_type(), string);
        let accepted = call_fact(call);
        let Some(UnitTypeKind::Function { parameters, .. }) =
            types.get(accepted.arguments()[1].parameter_type())
        else {
            panic!("expected a contextual lambda parameter");
        };
        assert_eq!(
            types.get(parameters[0].ty()),
            Some(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![long],
            })
        );
        assert!(matches!(
            types.get(accepted.return_type()),
            Some(UnitTypeKind::Nominal { arguments, .. }) if arguments == &[long]
        ));
        typed.clone().validate().unwrap();
    });
}

#[test]
fn rejected_selected_call_discards_instance_only_types_and_keeps_argument_facts() {
    for (declaration, call, error_source, contextual_lambda) in [
        (
            "fun <T> choose(tag: T, value: String): Holder<T> = error(\"never\")",
            "choose(input, checked(input))",
            "checked(input)",
            false,
        ),
        (
            "class Factory<T> { fun choose(value: String): Holder<T> = error(\"never\") }",
            "Factory<Long>().choose(checked(input))",
            "checked(input)",
            false,
        ),
        (
            "class Factory<T> { fun choose(view: (List<T>) -> Int, value: String): Holder<T> = error(\"never\") }",
            "Factory<Long>().choose({ values -> values.size }, checked(input))",
            "checked(input)",
            true,
        ),
        (
            "class Factory<T> { fun choose(view: (List<T>) -> Int, value: Argument<T>): Holder<T> = error(\"never\") }",
            "Factory<Long>().choose({ values -> val bad: String = values.size; 0 }, checked(input))",
            "values.size",
            true,
        ),
    ] {
        let text = format!(
            "class Holder<T>(val payload: Int)\n             class Argument<T>(val item: T)\n             {declaration}\n             fun <T> checked(value: T): Argument<T> = Argument(value)\n             fun entry(input: Long): Unit {{\n                 val rejected = {call}\n                 val retained = checked(7)\n             }}"
        );
        check_storage_instances(&text, |sources, file, unit, typed| {
            let [error] = typed.diagnostics() else {
                panic!("{text}: {:?}", typed.diagnostics());
            };
            assert_eq!(error.code().to_string(), "L0084");
            assert_eq!(sources.slice(error.primary_span()), Ok(error_source));
            let nominal = |field_name| {
                typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|signature| signature.nominal())
                    .find(|nominal| {
                        nominal
                            .fields()
                            .iter()
                            .any(|field| sources.slice(field.span()).unwrap() == field_name)
                    })
                    .unwrap()
                    .declaration()
            };
            let long = typed.types().builtin(BuiltinType::Long).unwrap();
            let argument_type = typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: nominal("item"),
                    arguments: vec![long],
                })
                .expect("independently checked argument still has a canonical type");
            let call_fact = |text| {
                let expression =
                    UnitExpressionId::new(unit, expression_with_text(sources, file, text));
                typed
                    .calls()
                    .iter()
                    .find(|fact| fact.expression() == expression)
            };
            assert_eq!(
                call_fact("checked(input)").unwrap().return_type(),
                argument_type
            );
            assert!(call_fact("checked(7)").is_some());
            assert!(call_fact(call).is_none());
            assert!(
                typed
                    .types()
                    .find(&UnitTypeKind::Nominal {
                        declaration: nominal("payload"),
                        arguments: vec![long],
                    })
                    .is_none(),
                "only the rejected call needs Holder<Long>"
            );
            if contextual_lambda {
                let list = typed
                    .types()
                    .find(&UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::List,
                        arguments: vec![long],
                    })
                    .expect("lambda context was checked");
                assert!(typed.body_symbol_types().values().any(|&ty| ty == list));
            }
            assert!(typed.clone().validate().is_err());
        });
    }
}
