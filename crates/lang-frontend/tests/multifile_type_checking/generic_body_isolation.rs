//! SPEC-0276: source-qualified templates and committed trials own cache seeds.
use super::*;

const PROBE: &str =
    "fun <T> probe(own item: T): Int { val values = listOf(item); return values.size }";

#[test]
fn unit_generic_body_source_identity_reverse_inputs_and_repeated_calls() {
    let mut canonical = None;
    for repeated in [false, true] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "provider.ko",
            "package p\nfun <T> probe(own item: T): Int {\n\
             val values = arrayOf(item)\nreturn values.size\n}",
        );
        let calls = if repeated {
            "p.probe<Int>(1)\np.probe(1)\np.probe(1)\n\
             probe<String>(\"word\")\nprobe(\"word\")\nprobe(\"word\")"
        } else {
            "p.probe<Int>(1)\np.probe(1)\n\
             probe<String>(\"word\")\nprobe(\"word\")"
        };
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "consumer.ko",
            &format!("package q\n{PROBE}\nfun entry(): Unit {{\n{calls}\n}}"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let reverse_names = validated_names(&sources, &reversed, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        let reverse =
            check_compilation_unit_types(&sources, &reversed, &reverse_names, &environment)
                .unwrap();
        assert_valid_and_stable(&typed, &reverse);
        assert!(typed.is_same_analysis(&typed.clone()));
        assert!(!typed.is_same_analysis(&reverse));
        assert!(typed.is_compatible_with(&sources, &reversed, &names, &environment));
        assert!(!typed.is_compatible_with(&sources, &inputs, &reverse_names, &environment));

        let provider_unit = source_unit(&names, provider_source);
        let consumer_unit = source_unit(&names, consumer_source);
        let provider_probe = source_declaration(&names, provider_unit, "probe");
        let consumer_probe = source_declaration(&names, consumer_unit, "probe");
        assert_ne!(provider_probe, consumer_probe);
        assert_template(
            &typed,
            &names,
            provider_unit,
            "probe",
            IntrinsicTypeConstructor::Array,
        );
        assert_template(
            &typed,
            &names,
            consumer_unit,
            "probe",
            IntrinsicTypeConstructor::List,
        );
        let provider_t = typed
            .signatures()
            .declaration(provider_probe)
            .unwrap()
            .callable()
            .unwrap()
            .type_parameters()[0];
        let consumer_t = typed
            .signatures()
            .declaration(consumer_probe)
            .unwrap()
            .callable()
            .unwrap()
            .type_parameters()[0];
        assert_ne!(
            provider_t, consumer_t,
            "same spelling T does not share identity"
        );
        assert_eq!(provider_t.source_unit(), provider_unit);
        assert_eq!(consumer_t.source_unit(), consumer_unit);

        for (target, builtin, count) in [
            (
                provider_probe,
                BuiltinType::Int,
                if repeated { 3 } else { 2 },
            ),
            (
                consumer_probe,
                BuiltinType::String,
                if repeated { 3 } else { 2 },
            ),
        ] {
            let actual = typed.types().builtin(builtin).unwrap();
            let calls = typed
                .calls()
                .iter()
                .filter(|call| call.target() == UnitCallTarget::Declaration(target))
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), count);
            assert!(calls.iter().all(|call| {
                call.expression().source_unit() == consumer_unit
                    && call.instance().type_arguments() == [actual]
            }));
        }
        for (constructor, builtin, present) in [
            (IntrinsicTypeConstructor::Array, BuiltinType::Int, true),
            (IntrinsicTypeConstructor::List, BuiltinType::String, true),
            (IntrinsicTypeConstructor::Array, BuiltinType::String, false),
            (IntrinsicTypeConstructor::List, BuiltinType::Int, false),
        ] {
            assert_eq!(
                typed
                    .types()
                    .find(&intrinsic(&typed, constructor, builtin))
                    .is_some(),
                present
            );
        }
        // Repeating the same actual calls changes source facts, but cannot
        // append a duplicate canonical type or another speculative instance.
        if let Some(previous) = &canonical {
            assert_eq!(typed.types(), previous);
        }
        canonical = Some(typed.types().clone());
    }
}

#[test]
fn unit_generic_body_uncalled_symbolic_helpers_do_not_guess_int() {
    for dormant in [
        "",
        "fun <T> dormant(own item: T): Int = probe(item)",
        "fun <T> dormant(own item: T): Int = probe<T>(item)",
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "symbolic.ko",
            &format!("package p\n{PROBE}\n{dormant}\nfun entry(): Int = 0"),
        );
        let inputs = [SourceUnitInput::new("root", "p/symbolic.ko", source, &file)];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        typed
            .clone()
            .validate()
            .expect("uncalled generic templates are valid");
        assert_template(
            &typed,
            &names,
            source_unit(&names, source),
            "probe",
            IntrinsicTypeConstructor::List,
        );
        for builtin in [BuiltinType::Int, BuiltinType::String] {
            assert_eq!(
                typed
                    .types()
                    .find(&intrinsic(&typed, IntrinsicTypeConstructor::List, builtin)),
                None
            );
        }
        for call in typed.calls() {
            assert!(matches!(
                typed.types().get(call.instance().type_arguments()[0]),
                Some(UnitTypeKind::TypeParameter(_))
            ));
        }
    }
}

#[test]
fn unit_generic_body_uncalled_generic_closed_call_is_a_real_seed() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "closed.ko",
        &format!(
            "package p\n{PROBE}\n\
            fun <U> dormant(): Int = probe<Int>(1)\nfun entry(): Int = 0"
        ),
    );
    let inputs = [SourceUnitInput::new("root", "p/closed.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    typed
        .clone()
        .validate()
        .expect("the closed static call is valid even in dormant<U>");
    assert_template(
        &typed,
        &names,
        source_unit(&names, source),
        "probe",
        IntrinsicTypeConstructor::List,
    );
    let [call] = typed.calls() else {
        panic!("only dormant has an ordinary source call")
    };
    assert_eq!(
        call.target(),
        UnitCallTarget::Declaration(declaration(&names, "probe"))
    );
    assert_eq!(
        call.instance().type_arguments(),
        &[typed.types().builtin(BuiltinType::Int).unwrap()]
    );
    assert!(
        typed
            .types()
            .find(&intrinsic(
                &typed,
                IntrinsicTypeConstructor::List,
                BuiltinType::Int
            ))
            .is_some()
    );
    assert_eq!(
        typed.types().find(&intrinsic(
            &typed,
            IntrinsicTypeConstructor::List,
            BuiltinType::String
        )),
        None
    );
}

#[test]
fn unit_generic_body_selected_lambda_trial_seeds_only_committed_actuals() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "provider.ko",
        &format!(
            "package p\n{PROBE}\n\
            fun resolve(callback: (Int) -> Int): Int = 1\n\
            fun resolve(callback: (String) -> String): String = \"text\"\n\
            fun intResult(input: Int): Int = input"
        ),
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "consumer.ko",
        "package p\nfun entry(): Unit {\n\
            val selected = resolve({ item -> probe(item); intResult(item) })\n}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "p/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reversed, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
    let reverse =
        check_compilation_unit_types(&sources, &reversed, &reverse_names, &environment).unwrap();
    assert_valid_and_stable(&typed, &reverse);
    assert_template(
        &typed,
        &names,
        source_unit(&names, provider_source),
        "probe",
        IntrinsicTypeConstructor::List,
    );
    let target = declaration(&names, "probe");
    let calls = typed
        .calls()
        .iter()
        .filter(|call| call.target() == UnitCallTarget::Declaration(target))
        .collect::<Vec<_>>();
    assert_eq!(
        calls.len(),
        1,
        "the rejected String candidate leaves no call seed"
    );
    let int = typed.types().builtin(BuiltinType::Int).unwrap();
    assert_eq!(calls[0].instance().type_arguments(), &[int]);
    assert_eq!(
        calls[0].expression().source_unit(),
        source_unit(&names, consumer_source)
    );
    let item = symbol_named(&typed, &names, source_unit(&names, consumer_source), "item");
    assert_eq!(typed.symbol_type(item), Some(int));
    assert_eq!(typed.body_parameter_mode(item), Some(ParameterMode::Borrow));
    assert!(
        typed
            .types()
            .find(&intrinsic(
                &typed,
                IntrinsicTypeConstructor::List,
                BuiltinType::Int
            ))
            .is_some()
    );
    assert_eq!(
        typed.types().find(&intrinsic(
            &typed,
            IntrinsicTypeConstructor::List,
            BuiltinType::String
        )),
        None
    );
}

#[test]
fn unit_generic_body_failed_and_ambiguous_trials_leak_no_seed() {
    for (tail, code, message) in [
        (
            "target = true",
            "L0123",
            "no overload matches the call arguments",
        ),
        (
            "target = 1",
            "L0124",
            "call remains ambiguous after argument type checking",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "provider.ko",
            &format!(
                "package p\n{PROBE}\n\
                fun resolve(callback: (Int) -> Unit): Int = 1\n\
                fun resolve(callback: (String) -> Unit): String = \"text\""
            ),
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "consumer.ko",
            &format!(
                "package p\nfun entry(inout target: Int): Unit {{\n\
                val rejected = resolve({{ item -> probe<Int>(1); {tail} }})\n}}"
            ),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "p/consumer.ko", consumer_source, &consumer),
        ];
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let reverse_names = validated_names(&sources, &reversed, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        let reverse =
            check_compilation_unit_types(&sources, &reversed, &reverse_names, &environment)
                .unwrap();
        assert_eq!(typed.diagnostics(), reverse.diagnostics());
        assert_eq!(typed.calls(), reverse.calls());
        assert_eq!(typed.expression_types(), reverse.expression_types());
        assert_eq!(typed.body_symbol_types(), reverse.body_symbol_types());
        assert_eq!(typed.body_parameter_modes(), reverse.body_parameter_modes());
        assert_rejected_cache(&typed, &sources, consumer_source, code, "resolve");
        assert_eq!(typed.body_diagnostics()[0].message(), message);
        assert!(
            typed.calls().is_empty(),
            "all nested candidate call facts roll back"
        );
        if code == "L0124" {
            let labels = typed.body_diagnostics()[0]
                .details()
                .iter()
                .filter_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => Some(label),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(labels.len(), 2);
            for label in &labels {
                assert_eq!(label.span().source_id(), provider_source);
                assert_eq!(sources.slice(label.span()), Ok("resolve"));
                assert_eq!(label.message(), "matching callable declared here");
            }
            assert!(labels[0].span().start() < labels[1].span().start());
        }
        assert_template(
            &typed,
            &names,
            source_unit(&names, provider_source),
            "probe",
            IntrinsicTypeConstructor::List,
        );
    }
}

#[test]
fn unit_generic_body_lambda_error_and_final_expected_type_skip_cache() {
    for (uses, primary) in [
        (
            "fun entry(): Unit { val rejected = take({ probe<Int>(1); true }) }",
            "true",
        ),
        (
            "fun entry(): Unit { val wrong: String = take({ probe<Int>(1) }) }",
            "take({ probe<Int>(1) })",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "provider.ko",
            &format!("package p\n{PROBE}\nfun take(callback: () -> Int): Int = 0"),
        );
        let (consumer_source, consumer) =
            parsed(&mut sources, "consumer.ko", &format!("package p\n{uses}"));
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "p/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        assert_rejected_cache(&typed, &sources, consumer_source, "L0084", primary);
        assert_template(
            &typed,
            &names,
            source_unit(&names, provider_source),
            "probe",
            IntrinsicTypeConstructor::List,
        );
        // Recovery may retain this valid nested call. Its real Int argument
        // must still not start canonical publication in an erroneous unit.
        let probe = declaration(&names, "probe");
        assert!(typed.calls().iter().any(|call| {
            call.target() == UnitCallTarget::Declaration(probe)
                && call.instance().type_arguments()
                    == [typed.types().builtin(BuiltinType::Int).unwrap()]
        }));
    }
}

fn intrinsic(
    typed: &CompilationUnitTypes,
    constructor: IntrinsicTypeConstructor,
    builtin: BuiltinType,
) -> UnitTypeKind {
    UnitTypeKind::Intrinsic {
        constructor,
        arguments: vec![typed.types().builtin(builtin).unwrap()],
    }
}

fn source_declaration(
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    name: &str,
) -> DeclarationId {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.source_unit() == source && declaration.name() == name)
        .expect("the source-qualified declaration exists")
        .id()
}

fn assert_template(
    typed: &CompilationUnitTypes,
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    name: &str,
    constructor: IntrinsicTypeConstructor,
) {
    let target = source_declaration(names, source, name);
    let callable = typed
        .signatures()
        .declaration(target)
        .unwrap()
        .callable()
        .unwrap();
    let symbol = callable.type_parameters()[0];
    assert_eq!(symbol.source_unit(), source);
    let parameter = typed
        .types()
        .find(&UnitTypeKind::TypeParameter(symbol))
        .unwrap();
    let facts = typed
        .container_constructions()
        .iter()
        .filter(|fact| fact.expression().source_unit() == source)
        .collect::<Vec<_>>();
    assert_eq!(facts.len(), 1);
    let kind = UnitTypeKind::Intrinsic {
        constructor,
        arguments: vec![parameter],
    };
    assert_eq!(typed.types().get(facts[0].container_type()), Some(&kind));
    assert_eq!(facts[0].element_type(), parameter);
    assert_eq!(
        typed.expression_type(facts[0].expression()),
        Some(facts[0].container_type())
    );
    assert_eq!(
        typed.symbol_type(symbol_named(typed, names, source, "values")),
        Some(facts[0].container_type())
    );
}

fn assert_valid_and_stable(typed: &CompilationUnitTypes, reverse: &CompilationUnitTypes) {
    for product in [typed, reverse] {
        assert!(
            product.diagnostics().is_empty(),
            "{:?}",
            product.diagnostics()
        );
        product
            .clone()
            .validate()
            .expect("every successful candidate validates");
    }
    assert_eq!(typed.types(), reverse.types());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.type_ref_types(), reverse.type_ref_types());
    assert_eq!(typed.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(typed.calls(), reverse.calls());
    assert_eq!(
        typed.container_constructions(),
        reverse.container_constructions()
    );
    assert_eq!(typed.diagnostics(), reverse.diagnostics());
}

fn assert_rejected_cache(
    typed: &CompilationUnitTypes,
    sources: &SourceMap,
    source: SourceId,
    code: &str,
    primary: &str,
) {
    let [diagnostic] = typed.body_diagnostics() else {
        panic!("exactly one original rejection: {:?}", typed.diagnostics())
    };
    assert_eq!(diagnostic.code().to_string(), code);
    assert_eq!(diagnostic.primary_span().source_id(), source);
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok(primary));
    for builtin in [BuiltinType::Int, BuiltinType::String] {
        assert_eq!(
            typed
                .types()
                .find(&intrinsic(typed, IntrinsicTypeConstructor::List, builtin)),
            None,
            "failed trials and final body errors must not publish a body-only canonical"
        );
    }
    assert!(typed.clone().validate().is_err());
}
