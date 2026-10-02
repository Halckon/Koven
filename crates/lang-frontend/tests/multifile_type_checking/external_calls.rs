use super::*;

#[test]
fn external_and_function_value_calls_publish_targets_modes_effects_and_stable_facts() {
    let mut names_environment = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names_environment
                .declare_type(builtin.name())
                .expect("unique builtin"),
            builtin,
        )
    });
    let consume = names_environment
        .declare_function("consumeExternal")
        .expect("consume external");
    let inspect = names_environment
        .declare_function("inspectExternal")
        .expect("inspect external");
    let send = names_environment
        .declare_function("sendExternal")
        .expect("send external");
    let stop = names_environment
        .declare_function("stopExternal")
        .expect("stop external");
    let print = names_environment
        .declare_function("printExternal")
        .expect("print external");
    let choose_int = names_environment
        .declare_function("chooseExternal")
        .expect("first external overload");
    let choose_string = names_environment
        .declare_function("chooseExternal")
        .expect("second external overload");
    let external_callback = names_environment
        .declare_value("externalCallback")
        .expect("external function value");
    let mut type_environment = TypeEnvironment::new(&names_environment);
    for (symbol, builtin) in builtins {
        type_environment
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    {
        let mut bind_function = |symbol, mode, parameter, result, effects| {
            type_environment
                .bind_function(
                    symbol,
                    EnvironmentFunction {
                        parameters: vec![EnvironmentParameter {
                            mode,
                            ty: EnvironmentType::Builtin(parameter),
                        }],
                        return_type: EnvironmentType::Builtin(result),
                        effects,
                    },
                )
                .expect("valid external function binding");
        };
        bind_function(
            consume,
            ParameterMode::Value,
            BuiltinType::Int,
            BuiltinType::Long,
            Vec::new(),
        );
        bind_function(
            inspect,
            ParameterMode::Borrow,
            BuiltinType::Int,
            BuiltinType::Long,
            Vec::new(),
        );
        bind_function(
            send,
            ParameterMode::Value,
            BuiltinType::String,
            BuiltinType::Unit,
            vec![EnvironmentFunctionEffect::CrossThreadTransfer { parameter: 0 }],
        );
        bind_function(
            stop,
            ParameterMode::Borrow,
            BuiltinType::String,
            BuiltinType::Nothing,
            vec![EnvironmentFunctionEffect::Abort],
        );
        bind_function(
            print,
            ParameterMode::Borrow,
            BuiltinType::String,
            BuiltinType::Unit,
            vec![EnvironmentFunctionEffect::PrintLine],
        );
        bind_function(
            choose_int,
            ParameterMode::Borrow,
            BuiltinType::Int,
            BuiltinType::Long,
            Vec::new(),
        );
        bind_function(
            choose_string,
            ParameterMode::Borrow,
            BuiltinType::String,
            BuiltinType::String,
            Vec::new(),
        );
    }
    type_environment
        .bind_value(
            external_callback,
            EnvironmentType::Function {
                move_only: false,
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::Int),
                }],
                return_type: Box::new(EnvironmentType::Builtin(BuiltinType::Long)),
            },
        )
        .expect("external function value binding");

    let mut sources = SourceMap::new();
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(callback: move (borrow Int) -> Long): Long {\n\
             val consumed = consumeExternal(1)\n\
             val inspected = inspectExternal(2)\n\
             val selected = chooseExternal(3)\n\
             val invoked = callback(4)\n\
             val externalInvoked = externalCallback(5)\n\
             val sent = sendExternal(\"payload\")\n\
             val printed = printExternal(\"line\")\n\
             return consumed + inspected + selected + invoked + externalInvoked\n\
         }\n\
         fun halt(): Unit { stopExternal(\"stop\") }",
    );
    let (stable_source, stable) = parsed(
        &mut sources,
        "stable.ko",
        "package p\nfun stable(): Int = 1",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/stable.ko", stable_source, &stable),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let forward_names = validated_names(&sources, &forward_inputs, &names_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &names_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("external and function-value calls succeed");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed external and function-value calls succeed");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    let uses_unit = source_unit(&forward_names, uses_source);
    let call = |text: &str| {
        forward
            .call(UnitExpressionId::new(
                uses_unit,
                expression_with_text(&sources, &uses, text),
            ))
            .expect("call descriptor")
    };
    for (text, target, mode) in [
        (
            "consumeExternal(1)",
            UnitCallTarget::External(consume),
            ParameterMode::Value,
        ),
        (
            "inspectExternal(2)",
            UnitCallTarget::External(inspect),
            ParameterMode::Borrow,
        ),
        (
            "chooseExternal(3)",
            UnitCallTarget::External(choose_int),
            ParameterMode::Borrow,
        ),
    ] {
        let descriptor = call(text);
        assert_eq!(descriptor.target(), target);
        assert_eq!(descriptor.arguments()[0].mode(), mode);
        assert!(descriptor.instance().type_arguments().is_empty());
    }
    for text in ["callback(4)", "externalCallback(5)"] {
        assert_eq!(call(text).target(), UnitCallTarget::FunctionValue);
    }
    assert!(call("sendExternal(\"payload\")").arguments()[0].crosses_thread());
    assert!(call("printExternal(\"line\")").prints_line());
    assert!(call("stopExternal(\"stop\")").aborts());
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(
                uses_unit,
                expression_with_text(&sources, &uses, "consumeExternal"),
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Function { parameters, return_type, .. })
            if parameters.len() == 1
                && parameters[0].mode() == ParameterMode::Value
                && forward.types().get(*return_type)
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Long))
    ));
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(
                uses_unit,
                expression_with_text(&sources, &uses, "callback"),
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Function { move_only, parameters, return_type })
            if *move_only
                && parameters.len() == 1
                && parameters[0].mode() == ParameterMode::Borrow
                && forward.types().get(*return_type)
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Long))
    ));
}

#[test]
fn invalid_external_and_function_value_calls_keep_precise_diagnostics_and_deferred_recovery() {
    let mut names_environment = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names_environment
                .declare_type(builtin.name())
                .expect("unique builtin"),
            builtin,
        )
    });
    let take = names_environment
        .declare_function("takeExternal")
        .expect("external function");
    let pick_int = names_environment
        .declare_function("pickExternal")
        .expect("first overload");
    let pick_string = names_environment
        .declare_function("pickExternal")
        .expect("second overload");
    let print = names_environment
        .declare_function("printExternal")
        .expect("effectful external function");
    names_environment
        .declare_function("unboundExternal")
        .expect("unbound external identity");
    let mut type_environment = TypeEnvironment::new(&names_environment);
    for (symbol, builtin) in builtins {
        type_environment
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, parameter, result) in [
        (take, BuiltinType::Int, BuiltinType::Int),
        (pick_int, BuiltinType::Int, BuiltinType::Int),
        (pick_string, BuiltinType::String, BuiltinType::String),
    ] {
        type_environment
            .bind_function(
                symbol,
                EnvironmentFunction {
                    parameters: vec![EnvironmentParameter {
                        mode: ParameterMode::Borrow,
                        ty: EnvironmentType::Builtin(parameter),
                    }],
                    return_type: EnvironmentType::Builtin(result),
                    effects: Vec::new(),
                },
            )
            .expect("external binding");
    }
    type_environment
        .bind_function(
            print,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::PrintLine],
            },
        )
        .expect("effectful external binding");

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-calls.ko",
        "package p\n\
         fun bad(callback: (borrow Int) -> Int, number: Int): Unit {\n\
             val named = callback(input = 1)\n\
             val nonCallable = number()\n\
             val mismatch = takeExternal(true)\n\
             val missing = takeExternal()\n\
             val noOverload = pickExternal(true)\n\
             val genericExternal = takeExternal<Int>(1)\n\
             val genericCallback = callback<Int>(1)\n\
             val genericEffect = printExternal<String>(\"line\")\n\
             val unbound = unboundExternal(1)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/invalid-calls.ko",
        source,
        &file,
    )];
    let names = validated_names(&sources, &inputs, &names_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("call failures remain in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0120", "L0119", "L0084", "L0121", "L0123"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["input", "number", "true", "takeExternal()", "pickExternal"]
    );
    assert!(typed.calls().is_empty());
    let unit = source_unit(&names, source);
    for text in [
        "takeExternal<Int>(1)",
        "callback<Int>(1)",
        "printExternal<String>(\"line\")",
    ] {
        assert!(matches!(
            typed
                .expression_type(UnitExpressionId::new(
                    unit,
                    expression_with_text(&sources, &file, text),
                ))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::Call))
        ));
    }
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &file, "unboundExternal(1)"),
            ))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(typed.validate().is_err());
}

#[test]
fn partial_external_overloads_and_nested_deferred_calls_never_publish_error_facts() {
    let mut names_environment = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names_environment
                .declare_type(builtin.name())
                .expect("unique builtin"),
            builtin,
        )
    });
    let partial_bound = names_environment
        .declare_function("partialExternal")
        .expect("bound partial overload");
    names_environment
        .declare_function("partialExternal")
        .expect("unbound partial overload");
    let pick_int = names_environment
        .declare_function("pickExternal")
        .expect("integer overload");
    let pick_string = names_environment
        .declare_function("pickExternal")
        .expect("string overload");
    names_environment
        .declare_function("unboundExternal")
        .expect("unbound nested call");
    let mut type_environment = TypeEnvironment::new(&names_environment);
    for (symbol, builtin) in builtins {
        type_environment
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, parameter, result) in [
        (partial_bound, BuiltinType::Int, BuiltinType::Int),
        (pick_int, BuiltinType::Int, BuiltinType::Int),
        (pick_string, BuiltinType::String, BuiltinType::String),
    ] {
        type_environment
            .bind_function(
                symbol,
                EnvironmentFunction {
                    parameters: vec![EnvironmentParameter {
                        mode: ParameterMode::Borrow,
                        ty: EnvironmentType::Builtin(parameter),
                    }],
                    return_type: EnvironmentType::Builtin(result),
                    effects: Vec::new(),
                },
            )
            .expect("external function binding");
    }

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "deferred-calls.ko",
        "package p\n\
         fun deferred(): Unit {\n\
             val partial = partialExternal(1)\n\
             val nested = pickExternal(unboundExternal())\n\
             val lambda = resolve({ item -> unboundExternal() })\n\
         }\n\
         fun resolve(callback: (borrow Int) -> Int): Int = 1\n\
         fun resolve(callback: (borrow String) -> String): String = \"resolved\"",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/deferred-calls.ko",
        source,
        &file,
    )];
    let names = validated_names(&sources, &inputs, &names_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("deferred external calls keep a recovery product");
    let unit = source_unit(&names, source);
    let expression_kind = |text| {
        typed
            .expression_type(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &file, text),
            ))
            .and_then(|ty| typed.types().get(ty))
    };

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(matches!(
        expression_kind("partialExternal(1)"),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(matches!(
        expression_kind("unboundExternal()"),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(matches!(
        expression_kind("pickExternal(unboundExternal())"),
        Some(UnitTypeKind::Deferred(DeferredReason::Call))
    ));
    assert!(matches!(
        expression_kind("resolve({ item -> unboundExternal() })"),
        Some(UnitTypeKind::Deferred(DeferredReason::Call))
    ));
    assert!(typed.calls().is_empty());
    assert!(typed.validate().is_ok());
}

#[test]
fn effectful_external_function_values_fail_loud_until_effect_identity_is_preserved() {
    for (name, text) in [
        (
            "p/grouped-effect.ko",
            "package p\nfun bad(): Unit { (println)(\"line\") }",
        ),
        (
            "p/aliased-effect.ko",
            "package p\nfun bad(): Unit { val output = println }",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, name, text);
        let inputs = [SourceUnitInput::new("root", name, source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let error = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect_err("effectful external function values must not erase compiler-bound effects");
        let lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(span) = error
        else {
            panic!("expected UnsupportedBody, got {error:?}");
        };
        assert_eq!(sources.slice(span), Ok("println"));
    }
}
