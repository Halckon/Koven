use super::*;

#[test]
fn cross_file_lambdas_publish_expected_contract_and_callable_boundaries() {
    let mut sources = SourceMap::new();
    let (callee_source, callee) = parsed(
        &mut sources,
        "callee.ko",
        "package p\nfun apply(callback: (borrow Int) -> Int): Unit",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun lambdas(): Int {\n\
             val reader: (borrow Int) -> Int = { borrowed -> borrowed }\n\
             val owner: (own Int) -> Int = { owned -> owned }\n\
             val writer: (inout Int) -> Int = { changed -> changed }\n\
             val moved: move (borrow Int) -> Int = move { captured -> captured }\n\
             val inferred = { 1 }\n\
             val boundary: () -> Unit = {\n\
                 loop { break }\n\
                 return\n\
             }\n\
             apply({ applied -> applied })\n\
             val implicit = apply { it }\n\
             return 1\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/callee.ko", callee_source, &callee),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward lambda unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse lambda unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward.body_parameter_modes(),
        reverse.body_parameter_modes()
    );
    let uses_unit = source_unit(&forward_names, uses_source);
    for (name, mode) in [
        ("borrowed", ParameterMode::Borrow),
        ("owned", ParameterMode::Value),
        ("changed", ParameterMode::Inout),
        ("captured", ParameterMode::Borrow),
        ("applied", ParameterMode::Borrow),
    ] {
        let symbol = symbol_named(&forward, &forward_names, uses_unit, name);
        assert_eq!(forward.body_parameter_mode(symbol), Some(mode));
        assert_eq!(
            forward
                .symbol_type(symbol)
                .and_then(|ty| forward.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
    assert!(matches!(
        forward
            .symbol_type(symbol_named(
                &forward,
                &forward_names,
                uses_unit,
                "inferred",
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Function {
            move_only: false,
            parameters,
            return_type,
        }) if parameters.is_empty()
            && forward.types().get(*return_type)
                == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    let implicit_local = forward_names.names().source_units()[uses_unit.index()]
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "it")
        .find_map(|symbol| {
            forward.body_symbol_types().keys().find(|candidate| {
                candidate.source_unit() == uses_unit
                    && candidate.symbol() == symbol.id()
                    && forward.body_parameter_mode(**candidate) == Some(ParameterMode::Borrow)
            })
        })
        .copied()
        .expect("activated implicit it symbol");
    assert_eq!(
        forward
            .symbol_type(implicit_local)
            .and_then(|ty| forward.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
}

#[test]
fn unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-lambdas.ko",
        "package p\n\
         fun takes(callback: (borrow Int) -> Int): Unit\n\
         fun invalid(): Unit {\n\
             val wrongMove: move (Int) -> Int = { item -> item }\n\
             val wrongArity: (Int) -> Int = { -> 1 }\n\
             val uninferred = { unknown -> unknown }\n\
             loop {\n\
                 val nested: () -> Unit = { break }\n\
                 break\n\
             }\n\
             val wrongReturn: () -> Unit = { return 1 }\n\
             takes(1)\n\
             val rejected = takes({ wrong -> true })\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/invalid-lambdas.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("lambda failures stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0084", "L0084", "L0083", "L0142", "L0087", "L0084", "L0084"
        ]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["->", "->", "->", "break", "1", "1", "true"]
    );
    assert!(
        typed.body_diagnostics()[5]
            .details()
            .iter()
            .any(|detail| matches!(
                detail,
                DiagnosticDetail::Label(label)
                    if sources.slice(label.span()).ok()
                        == Some("callback: (borrow Int) -> Int")
            ))
    );
    assert_eq!(typed.calls().len(), 1);
    assert!(typed.validate().is_err());
}

#[test]
fn ordinary_expected_move_literals_keep_canonical_type_and_borrow_parameters() {
    let mut sources = SourceMap::new();
    let (api_source, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\nfun apply(callback: (Int) -> Boolean): Unit {}",
    );
    let (source, file) = parsed(
        &mut sources,
        "use.ko",
        "package p\n\
         fun make(own returnedLabel: String): (Int) -> Boolean = move { returnedIndex -> returnedLabel == \"return\" && returnedIndex == 0 }\n\
         fun use(own localLabel: String, own argumentLabel: String): Unit {\n\
             val contextual: (Int) -> Boolean = move { localIndex -> localLabel == \"local\" && localIndex == 0 }\n\
             apply(move { argumentIndex -> argumentLabel == \"argument\" && argumentIndex == 0 })\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/api.ko", api_source, &api),
        SourceUnitInput::new("root", "p/use.ko", source, &file),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unit types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let unit = source_unit(&names, source);
    let expected = typed
        .symbol_type(symbol_named(&typed, &names, unit, "contextual"))
        .expect("ordinary expected type");
    assert!(
        matches!(typed.types().get(expected), Some(UnitTypeKind::Function {
        move_only: false, parameters, return_type,
    }) if parameters.len() == 1 && parameters[0].mode() == ParameterMode::Borrow
        && typed.types().get(parameters[0].ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        && typed.types().get(*return_type) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean)))
    );
    let lambdas = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::Lambda { .. }).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(lambdas.len(), 3);
    for lambda in lambdas {
        assert_eq!(
            typed.expression_type(UnitExpressionId::new(unit, lambda)),
            Some(expected),
            "local, argument and return literals share the canonical expected Function"
        );
    }
    for name in ["returnedIndex", "localIndex", "argumentIndex"] {
        let symbol = symbol_named(&typed, &names, unit, name);
        assert_eq!(
            typed.body_parameter_mode(symbol),
            Some(ParameterMode::Borrow)
        );
        assert_eq!(
            typed
                .symbol_type(symbol)
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
    typed.validate().expect("complete contextual lambda types");
}

#[test]
fn contextual_move_literal_acceptance_preserves_named_identity_mode_and_arity_errors() {
    let mut sources = SourceMap::new();
    let (api_source, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\nfun take(callback: (Int) -> Int): Unit {}\nfun strong(callback: move (Int) -> Int): Unit {}",
    );
    let (source, file) = parsed(
        &mut sources,
        "use.ko",
        "package p\n\
         fun use(named: move (Int) -> Int, ordinary: (Int) -> Int): Unit {\n\
             val rejected: (Int) -> Int = named\n\
             take(named)\n\
             val wrongMode: (own Int) -> Int = ordinary\n\
             strong({ shared -> shared })\n\
             val wrongArity: (Int) -> Int = move { first, second -> first }\n\
             val inferred = move { 1 }\n\
             val wrongInferred: () -> Int = inferred\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/api.ko", api_source, &api),
        SourceUnitInput::new("root", "p/use.ko", source, &file),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unit types");
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084"; 6]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("primary"))
            .collect::<Vec<_>>(),
        ["named", "named", "ordinary", "->", "->", "inferred"]
    );
    let unit = source_unit(&names, source);
    for name in ["shared", "first", "second"] {
        let symbol = symbol_named(&typed, &names, unit, name);
        assert_eq!(
            typed.body_parameter_mode(symbol),
            None,
            "structurally rejected lambda must not publish an adopted parameter mode"
        );
    }
    assert!(
        matches!(typed.symbol_type(symbol_named(&typed, &names, unit, "inferred")).and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Function { move_only: true, parameters, return_type }) if parameters.is_empty()
            && typed.types().get(*return_type) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))),
        "uncontextualized move literal keeps its move Function identity"
    );
    assert!(typed.validate().is_err());
}
