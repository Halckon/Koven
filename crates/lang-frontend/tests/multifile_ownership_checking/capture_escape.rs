use super::*;

fn cross_thread_environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        )
    });
    let capabilities = [
        (
            names.declare_type("Copyable").expect("Copyable"),
            Capability::Copyable,
        ),
        (
            names.declare_type("Transferable").expect("Transferable"),
            Capability::Transferable,
        ),
    ];
    let dispatch = names.declare_function("dispatch").expect("dispatch");
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, capability) in capabilities {
        types
            .bind_capability(symbol, capability)
            .expect("capability binding");
    }
    types
        .bind_function(
            dispatch,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Value,
                    ty: EnvironmentType::Function {
                        move_only: true,
                        parameters: Vec::new(),
                        return_type: Box::new(EnvironmentType::Builtin(BuiltinType::Unit)),
                    },
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::CrossThreadTransfer { parameter: 0 }],
            },
        )
        .expect("dispatch binding");
    (names, types)
}

#[test]
fn implicit_member_call_in_lambda_captures_this_in_unit_product() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Counter(var n: Int) { borrow fun read(): Int = n\nborrow fun keep(): Unit { val f: () -> Int = { read() } } }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("ownership product");
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership
            .captures()
            .iter()
            .any(|capture| capture.source() == UnitClosureCaptureSource::This)
    );

    let mut escape_sources = SourceMap::new();
    let (escape_source, escape_parsed) = self::parsed(
        &mut escape_sources,
        "escape.ko",
        "class Counter(var n: Int) { borrow fun read(): Int = n\nborrow fun escape(): () -> Int = { read() } }",
    );
    let escape_inputs = [SourceUnitInput::new(
        "root",
        "escape.ko",
        escape_source,
        &escape_parsed,
    )];
    let escape_names = validated_names(&escape_sources, &escape_inputs, &name_environment);
    let escape_typed = validated_types(
        &escape_sources,
        &escape_inputs,
        &escape_names,
        &type_environment,
    );
    let escape_ownership = check_compilation_unit_ownership(
        &escape_sources,
        &escape_inputs,
        &escape_names,
        &type_environment,
        &escape_typed,
    )
    .expect("ownership product");
    assert_eq!(diagnostic_codes(&escape_ownership), ["L0137"]);

    let mut sources = SourceMap::new();
    let (source, parsed) = self::parsed(
        &mut sources,
        "main.ko",
        "class Resource { own fun consume(): Unit {}\nown fun bad(): Unit { val f: () -> Unit = { consume() } } }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("ownership product");
    assert_eq!(diagnostic_codes(&ownership), ["L0133"]);
}

#[test]
fn static_self_value_call_cannot_move_a_shared_this_capture() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface I { own fun consume(): Unit {}\nown fun bad(): Unit { val f: () -> Unit = { consume() } } }\nclass Resource: I {}",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("ownership product");
    assert_eq!(diagnostic_codes(&ownership), ["L0133"]);
    assert!(ownership.conditional_receiver_deliveries().is_empty());
}

#[test]
fn conditional_expression_body_rejects_borrowed_closure_escape() {
    for body in [
        "= if (flag) ({ read(x) }) else ({})",
        "= when (flag) { true -> ({ read(x) })\nelse -> ({}) }",
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun read(x: Int): Unit {{}}\nfun leak(flag: Boolean, x: Int): () -> Unit {body}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert_eq!(diagnostic_codes(&ownership), ["L0137"], "{body}");
    }
}

#[test]
fn elvis_expression_body_rejects_borrowed_closure_escape() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "val none: Nothing? = null\nfun read(xs: List<Int>): Unit {}\nfun leak(xs: List<Int>): () -> Unit = none ?: ({ read(xs) })",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("ownership product");
    assert_eq!(diagnostic_codes(&ownership), ["L0137"]);
}

#[test]
fn shared_this_capture_rejects_moved_outer_and_move_from_capture() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Resource {\n\
             own fun movedCapture(): Unit {\n\
                 val moved = this\n\
                 val closure: () -> Unit = { val observed = this }\n\
             }\n\
             fun moveFromCapture(): () -> Unit = { val observed = this }\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131", "L0137", "L0133"]);
    assert!(ownership.captures().is_empty());
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.drops().is_empty());
}

#[test]
fn failed_this_capture_rolls_back_earlier_symbol_capture_state() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Resource {\n\
             own fun invalid(own other: Resource): Unit {\n\
                 val moved = this\n\
                 val closure: () -> Unit = {\n\
                     val first = inspect(other)\n\
                     val second = this\n\
                 }\n\
                 val taken = take(other)\n\
             }\n\
         }\n\
         fun inspect(item: Resource): Unit {}\n\
         fun take(own item: Resource): Unit {}",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131"]);
    assert!(ownership.captures().is_empty());
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.drops().is_empty());
}

#[test]
fn failed_lambda_body_rolls_back_successful_capture_state() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Resource {}\n\
         fun take(own item: Resource): Unit {}\n\
         fun invalid(own other: Resource): Unit {\n\
             val closure: () -> Unit = { val invalid = take(other) }\n\
             val valid = take(other)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0133"]);
    assert!(ownership.captures().is_empty());
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.drops().is_empty());
}

#[test]
fn lambda_body_executes_ordinary_call_ownership_dataflow() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun consume(own message: String): Unit {}\n\
         fun read(message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(): Unit {\n\
             val callback: (own String) -> Unit = { message ->\n\
                 val consumed = p.consume(message)\n\
                 val read = p.read(message)\n\
             }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(ownership.diagnostics().len(), 1);
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0131");
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn closure_capture_inputs_use_unit_identity_types_and_stable_transferability() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nclass Resource {}\nfun inspect(item: Resource): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.inspect\n\
         fun captures(own resource: Resource, number: Int): Unit {\n\
             val shared: () -> Unit = {\n\
                 val first = number\n\
                 val second = inspect(resource)\n\
             }\n\
             val owned: move () -> Unit = move {\n\
                 val first = number\n\
                 val second = resource\n\
             }\n\
             val empty: () -> Unit = {}\n\
         }\n\
         class Holder(val resource: Resource) {\n\
             fun closure(): Unit { val f: () -> Unit = {\n\
                 val captured = inspect(resource)\n\
             } }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    let consumer_unit = source_unit(&names, consumer_source);
    let lambdas = consumer
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, expression)| {
            matches!(
                expression.payload(),
                lang_frontend::parser::Expression::Lambda { .. }
            )
            .then_some(UnitExpressionId::new(consumer_unit, id))
        })
        .collect::<Vec<_>>();
    assert_eq!(lambdas.len(), 4);
    let resource = symbol_named(&ownership, &names, consumer_unit, "resource");
    let number = symbol_named(&ownership, &names, consumer_unit, "number");
    let shared = ownership
        .captures_of(lambdas[0])
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(
        shared
            .iter()
            .map(|capture| capture.source())
            .collect::<Vec<_>>(),
        [
            UnitClosureCaptureSource::Symbol(number),
            UnitClosureCaptureSource::Symbol(resource),
        ]
    );
    assert!(shared.iter().all(|capture| {
        capture.mode() == ClosureCaptureMode::Shared
            && capture.effect() == ClosureCaptureEffect::Borrow
    }));
    assert_eq!(
        shared[0].ty(),
        typed
            .types()
            .symbol_type(number)
            .expect("number has a unit-global type")
    );
    assert_eq!(
        shared[1].ty(),
        typed
            .types()
            .symbol_type(resource)
            .expect("resource has a unit-global type")
    );
    let owned = ownership
        .captures_of(lambdas[1])
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(owned[0].source(), UnitClosureCaptureSource::Symbol(number));
    assert_eq!(owned[0].effect(), ClosureCaptureEffect::Copy);
    assert_eq!(
        owned[1].source(),
        UnitClosureCaptureSource::Symbol(resource)
    );
    assert_eq!(owned[1].effect(), ClosureCaptureEffect::Move);
    assert_eq!(
        ownership
            .closure(lambdas[0])
            .expect("shared closure")
            .transferability(),
        Transferability::NotTransferable
    );
    assert_eq!(
        ownership
            .closure(lambdas[1])
            .expect("owned closure")
            .transferability(),
        Transferability::Transferable
    );
    assert_eq!(
        ownership
            .closure(lambdas[2])
            .expect("empty closure")
            .transferability(),
        Transferability::Transferable
    );
    let this_captures = ownership
        .captures_of(lambdas[3])
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(this_captures.len(), 1);
    let this_capture = this_captures[0];
    assert_eq!(this_capture.source(), UnitClosureCaptureSource::This);
    assert_eq!(this_capture.mode(), ClosureCaptureMode::Shared);
    assert_eq!(
        ownership.transferability(this_capture.ty()),
        Some(Transferability::Transferable)
    );

    let reversed_inputs = [inputs[1], inputs[0]];
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reversed_typed = validated_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    );
    let reversed = check_compilation_unit_ownership(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
        &reversed_typed,
    )
    .expect("reversed ownership product");
    assert_eq!(ownership.captures(), reversed.captures());
    assert_eq!(ownership.closures(), reversed.closures());
}

#[test]
fn ownership_diagnostics_clear_capture_and_executable_facts_atomically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nclass Resource {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         fun invalid(own resource: Resource): Unit {\n\
             val closure: move () -> Unit = move { val captured = resource }\n\
             val first = resource\n\
             val second = resource\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(ownership.diagnostics().len(), 2);
    assert!(
        ownership
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code().to_string() == "L0131")
    );
    assert!(ownership.captures().is_empty());
    assert_eq!(ownership.closures().len(), 1);
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.rc_effects().is_empty());
    assert!(ownership.construction_plans().is_empty());
}

#[test]
fn closure_formation_matches_single_file_move_borrow_and_immutability_rules() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun inspect(item: Resource): Unit {}\n\
         fun take(own item: Resource): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.inspect\n\
         import p.take\n\
         fun moved(own item: Resource): Unit {\n\
             val closure: move () -> Unit = move { val captured = inspect(item) }\n\
             val after = inspect(item)\n\
         }\n\
         fun borrowed(item: Resource): Unit {\n\
             val closure: move () -> Unit = move { val captured = inspect(item) }\n\
         }\n\
         fun moveInside(own item: Resource): Unit {\n\
             val closure: () -> Unit = { val captured = take(item) }\n\
         }\n\
         fun assignInside(): Unit {\n\
             var number = 1\n\
             val closure: () -> Unit = { number = 2 }\n\
         }\n\
         class Holder(val item: Resource) {\n\
             fun receiver(): move () -> Unit = move { val captured = inspect(item) }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(
        diagnostic_codes(&ownership),
        ["L0131", "L0138", "L0133", "L0135", "L0138"]
    );
    assert!(ownership.captures().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());

    let reversed_inputs = [inputs[1], inputs[0]];
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reversed_typed = validated_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    );
    let reversed = check_compilation_unit_ownership(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
        &reversed_typed,
    )
    .expect("reversed recovery ownership product");
    assert_eq!(ownership.diagnostics(), reversed.diagnostics());
}

#[test]
fn shared_capture_loan_ends_at_last_closure_use_and_still_blocks_earlier_move() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun inspect(item: Resource): Unit {}\n\
         fun take(own item: Resource): Unit {}\n\
         fun run(action: () -> Unit): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.inspect\n\
         import p.take\n\
         import p.run\n\
         fun released(own item: Resource): Unit {\n\
             val closure: () -> Unit = { val captured = inspect(item) }\n\
             val invoked = run(closure)\n\
             val moved = take(item)\n\
         }\n\
         fun direct(own item: Resource): Unit {\n\
             val invoked = run({ val captured = inspect(item) })\n\
             val moved = take(item)\n\
         }\n\
         fun conflict(own item: Resource): Unit {\n\
             val closure: () -> Unit = { val captured = inspect(item) }\n\
             val moved = take(item)\n\
             val invoked = run(closure)\n\
         }\n\
         class Holder(var item: Resource) {\n\
             fun conflictField(): Unit {\n\
                 val closure: () -> Unit = { val captured = inspect(item) }\n\
                 item = Resource()\n\
                 val invoked = run(closure)\n\
             }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0135", "L0134"]);
    assert!(ownership.captures().is_empty());
}

#[test]
fn borrowed_closure_cannot_escape_through_return_value_delivery_constructor_or_field() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun inspect(item: Resource): Unit {}\n\
         fun deliver(own callback: () -> Unit): Unit {}\n\
         class Envelope(val callback: () -> Unit)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.inspect\n\
         import p.deliver\n\
         import p.Envelope\n\
         fun returnIt(own item: Resource): () -> Unit {\n\
             val closure: () -> Unit = { val captured = inspect(item) }\n\
             return closure\n\
         }\n\
         fun passIt(own item: Resource): Unit {\n\
             val closure: () -> Unit = { val captured = inspect(item) }\n\
             val sent = deliver(closure)\n\
         }\n\
         fun constructIt(own item: Resource): Unit {\n\
             val closure: () -> Unit = { val captured = inspect(item) }\n\
             val envelope = Envelope(closure)\n\
         }\n\
         class Slot(var callback: () -> Unit) {\n\
             fun store(own item: Resource): Unit {\n\
                 val closure: () -> Unit = { val captured = inspect(item) }\n\
                 callback = closure\n\
             }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(
        diagnostic_codes(&ownership),
        ["L0137", "L0137", "L0137", "L0137"]
    );
    assert!(ownership.captures().is_empty());
    assert!(ownership.construction_plans().is_empty());
}

#[test]
fn compiler_bound_cross_thread_delivery_uses_unit_closure_transferability() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nclass Local(val callback: () -> Unit)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Local\n\
         fun invalid(own local: Local): Unit {\n\
             val sent = dispatch(move { val captured = local })\n\
         }\n\
         fun empty(): Unit {\n\
             val sent = dispatch(move {})\n\
         }\n\
         fun opaque(own callback: move () -> Unit): Unit {\n\
             val sent = dispatch(callback)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = cross_thread_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    assert!(
        typed
            .types()
            .calls()
            .iter()
            .filter(|call| !call.arguments().is_empty())
            .all(|call| call.arguments()[0].crosses_thread())
    );
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0139", "L0139"]);
    assert!(ownership.captures().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn ordinary_expected_move_literals_preserve_owned_capture_and_asap_drop() {
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
             apply(contextual)\n\
             apply(move { argumentIndex -> argumentLabel == \"argument\" && argumentIndex == 0 })\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/api.ko", api_source, &api),
        SourceUnitInput::new("root", "p/use.ko", source, &file),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership");
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert_eq!(
        ownership.loans().len(),
        2,
        "only callback arguments form shared loans; move capture owns its source"
    );
    let unit = source_unit(&names, source);
    let mut lambdas = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(
                node.payload(),
                lang_frontend::parser::Expression::Lambda { .. }
            )
            .then_some((node.span().start(), UnitExpressionId::new(unit, id)))
        })
        .collect::<Vec<_>>();
    lambdas.sort_by_key(|(start, _)| *start);
    assert_eq!(lambdas.len(), 3);
    for ((_, lambda), name) in
        lambdas
            .into_iter()
            .zip(["returnedLabel", "localLabel", "argumentLabel"])
    {
        let source = symbol_named(&ownership, &names, unit, name);
        let captures = ownership.captures_of(lambda).collect::<Vec<_>>();
        assert_eq!(captures.len(), 1);
        assert_eq!(
            captures[0].source(),
            UnitClosureCaptureSource::Symbol(source)
        );
        assert_eq!(captures[0].mode(), ClosureCaptureMode::Owned);
        assert_eq!(captures[0].effect(), ClosureCaptureEffect::Move);
        assert_eq!(
            typed.types().types().get(captures[0].ty()),
            Some(&UnitTypeKind::Builtin(BuiltinType::String))
        );
        assert_eq!(
            typed.types().copyability(captures[0].ty()),
            Copyability::MoveOnly
        );
        assert!(
            !ownership
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Named(source))
        );
        let capture_drops = ownership
            .drops()
            .iter()
            .filter(|drop| {
                matches!(drop.target(),
            UnitDropTarget::Captured { closure, source: UnitClosureCaptureSource::Symbol(captured) }
                if closure == lambda && captured == source)
            })
            .collect::<Vec<_>>();
        if name == "returnedLabel" {
            assert!(
                capture_drops.is_empty(),
                "returned environment transfers to the caller"
            );
        } else {
            assert_eq!(
                capture_drops.len(),
                1,
                "owned capture drops once with its closure"
            );
            let UnitDropPoint::CallReturn(call) = capture_drops[0].point() else {
                panic!("closure must drop after its final synchronous call");
            };
            let call_text = sources
                .slice(
                    file.ast()
                        .expressions()
                        .get(call.expression())
                        .expect("call")
                        .span(),
                )
                .expect("call text");
            let expected_call = if name == "localLabel" {
                "apply(contextual)"
            } else {
                "apply(move { argumentIndex -> argumentLabel == \"argument\" && argumentIndex == 0 })"
            };
            assert_eq!(
                call_text, expected_call,
                "capture cleanup belongs to its own final use"
            );
            let loan = ownership
                .loans()
                .iter()
                .find(|loan| loan.call() == call)
                .expect("callback borrow");
            assert_eq!(loan.kind(), LoanKind::Shared);
            if name == "localLabel" {
                let contextual = names.names().source_units()[unit.index()]
                    .resolution()
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.name() == "contextual")
                    .expect("local closure symbol")
                    .id();
                assert!(matches!(loan.target(), UnitLoanTarget::Place(place)
                    if place.root().source_unit() == unit && place.root().symbol() == contextual));
            } else {
                assert_eq!(loan.target(), &UnitLoanTarget::Temporary(lambda));
            }
            assert_eq!(
                loan.end_span(),
                file.ast()
                    .expressions()
                    .get(call.expression())
                    .expect("call")
                    .span()
            );
        }
    }
    ownership
        .validate()
        .expect("owned capture and ASAP plans validate");
}

#[test]
fn ordinary_expected_lambda_keeps_capture_escape_and_ownership_errors() {
    for (text, expected) in [
        (
            "fun invalid(label: String): (Int) -> Boolean = { index -> label == \"shared\" && index == 0 }",
            "L0137",
        ),
        (
            "fun invalid(label: String): (Int) -> Boolean = move { index -> label == \"borrowed\" && index == 0 }",
            "L0138",
        ),
        (
            "fun invalid(own label: String): Unit { val f: (Int) -> Boolean = move { index -> label == \"owned\" && index == 0 }\nval after = label == \"after\" }",
            "L0131",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "capture-boundaries.ko", text);
        let inputs = [SourceUnitInput::new(
            "root",
            "capture-boundaries.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("recovery ownership");
        assert_eq!(diagnostic_codes(&ownership), [expected], "{text}");
        assert!(ownership.captures().is_empty());
        assert!(ownership.loans().is_empty());
        assert!(
            ownership.drops().is_empty(),
            "ownership errors suppress executable drop plans"
        );
        assert!(ownership.validate().is_err());
    }
}
