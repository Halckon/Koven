//! SPEC-0198 compilation-unit ownership product integration tests.

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{
        NameEnvironment, SourceUnitId, SourceUnitInput, UnitSymbolId,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, CompilationUnitOwnership,
        ConstructionDeliveryKind, ConstructionRootKind, LoanKind, OwnershipBindingKind,
        OwnershipCheckingError, OwnershipDeferredReason, RcOwnershipEffectKind, Transferability,
        UnitCallArgumentOwnershipKind, UnitClosureCaptureSource, UnitDropPoint, UnitDropTarget,
        UnitLoanTarget, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
        UnitValueDeliveryKind, check_compilation_unit_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, Capability, Copyability, EnvironmentFunction, EnvironmentFunctionEffect,
        EnvironmentParameter, EnvironmentType, ParameterMode, TypeEnvironment,
        UnitCallReceiverOrigin, UnitExpressionId, UnitTypeKind, ValidatedCompilationUnitTypes,
        check_compilation_unit_types, standard_environments,
    },
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

fn validated_names<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> ValidatedCompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names")
}

fn validated_types(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
) -> ValidatedCompilationUnitTypes {
    check_compilation_unit_types(sources, inputs, names, environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid compilation-unit types")
}

fn source_unit(names: &ValidatedCompilationUnitNames, source: SourceId) -> SourceUnitId {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == source)
        .expect("source belongs to unit")
        .id()
}

fn expression_with_text(sources: &SourceMap, parsed: &ParsedFile, text: &str) -> ExpressionId {
    parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            sources
                .slice(expression.span())
                .is_ok_and(|actual| actual == text)
                .then_some(id)
        })
        .expect("expression text exists")
}

fn symbol_named(
    ownership: &CompilationUnitOwnership,
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    name: &str,
) -> UnitSymbolId {
    let resolution = names.names().source_units()[source.index()].resolution();
    let symbol = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .expect("symbol exists");
    ownership
        .bindings()
        .iter()
        .map(|binding| binding.symbol())
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol.id())
        .expect("source-qualified ownership binding exists")
}

fn diagnostic_codes(ownership: &CompilationUnitOwnership) -> Vec<String> {
    ownership
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

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
fn unit_parameter_bindings_cover_cross_file_member_and_lambda_modes() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun accept(own calleeOwned: String, calleeShared: String, inout calleeExclusive: String): Unit {}\n\
         class Worker {\n\
             fun run(own memberOwned: String, memberShared: String): Unit {}\n\
             companion object {\n\
                 fun configure(inout companionExclusive: String): Unit {}\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Worker\n\
         fun use(worker: Worker, own first: String, second: String, inout third: String, own fourth: String, fifth: String): Unit {\n\
             p.accept(calleeShared = second, calleeOwned = first, calleeExclusive = &third)\n\
             val memberResult = worker.run(fourth, fifth)\n\
             val callback: (own String, borrow String, inout String) -> Unit =\n\
                 { lambdaOwned, lambdaShared, lambdaExclusive -> }\n\
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

    let provider_unit = source_unit(&names, provider_source);
    let consumer_unit = source_unit(&names, consumer_source);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("unit Int type");
    assert_eq!(typed.types().copyability(int), Copyability::Copyable);
    let first = symbol_named(&ownership, &names, consumer_unit, "first");
    let first_type = typed
        .types()
        .symbol_type(first)
        .expect("String parameter type");
    assert_eq!(typed.types().copyability(first_type), Copyability::MoveOnly);
    for (source, name, expected) in [
        (provider_unit, "calleeOwned", OwnershipBindingKind::Owned),
        (provider_unit, "calleeShared", OwnershipBindingKind::Shared),
        (
            provider_unit,
            "calleeExclusive",
            OwnershipBindingKind::Exclusive,
        ),
        (provider_unit, "memberOwned", OwnershipBindingKind::Owned),
        (provider_unit, "memberShared", OwnershipBindingKind::Shared),
        (
            provider_unit,
            "companionExclusive",
            OwnershipBindingKind::Exclusive,
        ),
        (consumer_unit, "worker", OwnershipBindingKind::Shared),
        (consumer_unit, "first", OwnershipBindingKind::Owned),
        (consumer_unit, "second", OwnershipBindingKind::Shared),
        (consumer_unit, "third", OwnershipBindingKind::Exclusive),
        (consumer_unit, "fourth", OwnershipBindingKind::Owned),
        (consumer_unit, "fifth", OwnershipBindingKind::Shared),
        (consumer_unit, "lambdaOwned", OwnershipBindingKind::Owned),
        (consumer_unit, "lambdaShared", OwnershipBindingKind::Shared),
        (
            consumer_unit,
            "lambdaExclusive",
            OwnershipBindingKind::Exclusive,
        ),
    ] {
        let symbol = symbol_named(&ownership, &names, source, name);
        assert_eq!(ownership.binding_kind(symbol), Some(expected), "{name}");
        let descriptor = ownership
            .bindings()
            .iter()
            .find(|binding| binding.symbol() == symbol)
            .expect("binding descriptor");
        assert!(
            sources
                .slice(descriptor.declaration_span())
                .is_ok_and(|text| text.contains(name)),
            "{name} declaration span"
        );
    }
    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.loans().len(), 3);
    for (loan, (name, kind)) in ownership.loans().iter().zip([
        ("second", LoanKind::Shared),
        ("third", LoanKind::Exclusive),
        ("fifth", LoanKind::Shared),
    ]) {
        assert_eq!(loan.kind(), kind, "{name}");
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("{name} must establish a place loan");
        };
        assert_eq!(
            place.root(),
            symbol_named(&ownership, &names, consumer_unit, name)
        );
        assert!(place.fields().is_empty());
        assert_eq!(
            loan.end_span(),
            ownership
                .call_argument_contracts_for(loan.call())
                .next()
                .expect("loan call contract")
                .call_span()
        );
    }
    assert_eq!(ownership.value_deliveries().len(), 2);
    for (delivery, name) in ownership.value_deliveries().iter().zip(["first", "fourth"]) {
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move, "{name}");
        assert_eq!(
            delivery.place().map(|place| place.root()),
            Some(symbol_named(&ownership, &names, consumer_unit, name))
        );
    }
    assert!(ownership.is_compatible_with(&typed));
    assert!(ownership.is_same_analysis(&ownership.clone()));

    let accept_call = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(
            &sources,
            &consumer,
            "p.accept(calleeShared = second, calleeOwned = first, calleeExclusive = &third)",
        ),
    );
    let contracts = ownership
        .call_argument_contracts_for(accept_call)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(contracts.len(), 3);
    for (contract, (argument, parameter, parameter_index, kind)) in contracts.iter().zip([
        (
            "second",
            "calleeShared",
            1,
            UnitCallArgumentOwnershipKind::SharedLoan,
        ),
        (
            "first",
            "calleeOwned",
            0,
            UnitCallArgumentOwnershipKind::Value,
        ),
        (
            "third",
            "calleeExclusive",
            2,
            UnitCallArgumentOwnershipKind::ExclusiveLoan,
        ),
    ]) {
        assert_eq!(contract.call(), accept_call);
        assert_eq!(contract.parameter_index(), parameter_index);
        assert_eq!(contract.kind(), kind);
        assert!(!contract.crosses_thread());
        assert_eq!(
            sources
                .slice(contract.argument_span())
                .expect("argument span"),
            argument
        );
        assert_eq!(
            sources.slice(contract.call_span()).expect("call span"),
            "p.accept(calleeShared = second, calleeOwned = first, calleeExclusive = &third)"
        );
        assert!(
            contract
                .parameter_span()
                .and_then(|span| sources.slice(span).ok())
                .is_some_and(|text| text.contains(parameter)),
            "{parameter} declaration span"
        );
        assert_eq!(contract.argument().source_unit(), accept_call.source_unit());
        assert!(
            typed
                .types()
                .types()
                .get(contract.parameter_type())
                .is_some()
        );
    }

    let member_call = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "worker.run(fourth, fifth)"),
    );
    let member_contracts = ownership
        .call_argument_contracts_for(member_call)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(member_contracts.len(), 2);
    assert_eq!(
        member_contracts
            .iter()
            .map(|contract| contract.kind())
            .collect::<Vec<_>>(),
        [
            UnitCallArgumentOwnershipKind::Value,
            UnitCallArgumentOwnershipKind::SharedLoan,
        ]
    );
    for (contract, parameter) in member_contracts.iter().zip(["memberOwned", "memberShared"]) {
        assert!(
            contract
                .parameter_span()
                .and_then(|span| sources.slice(span).ok())
                .is_some_and(|text| text.contains(parameter)),
            "{parameter} member declaration span"
        );
    }

    let reversed_inputs = [inputs[1], inputs[0]];
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reversed_typed = validated_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    );
    let reversed_ownership = check_compilation_unit_ownership(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
        &reversed_typed,
    )
    .expect("reversed unit ownership product");
    assert_eq!(ownership.bindings(), reversed_ownership.bindings());
    assert_eq!(
        ownership.call_argument_contracts(),
        reversed_ownership.call_argument_contracts()
    );
    assert_eq!(ownership.diagnostics(), reversed_ownership.diagnostics());
    assert_eq!(ownership.loans(), reversed_ownership.loans());
    assert_eq!(
        ownership.value_deliveries(),
        reversed_ownership.value_deliveries()
    );
}

#[test]
fn unit_ownership_rejects_mixed_analysis_and_duplicate_inputs() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\nfun read(input: String): Unit {}",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("first ownership product");

    let second_names = validated_names(&sources, &inputs, &name_environment);
    assert!(matches!(
        check_compilation_unit_ownership(
            &sources,
            &inputs,
            &second_names,
            &type_environment,
            &typed,
        ),
        Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
    ));

    let second_typed = validated_types(&sources, &inputs, &names, &type_environment);
    assert!(!ownership.is_compatible_with(&second_typed));
    let duplicate_inputs = [inputs[0], inputs[0]];
    assert!(matches!(
        check_compilation_unit_ownership(
            &sources,
            &duplicate_inputs,
            &names,
            &type_environment,
            &typed,
        ),
        Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
    ));
}

#[test]
fn function_value_and_external_call_contracts_do_not_invent_source_parameters() {
    let mut sources = SourceMap::new();
    let (uses_source, uses) = parsed(
        &mut sources,
        "p/uses.ko",
        "package p\n\
         fun use(callback: (borrow String) -> Unit, message: String): Unit {\n\
             val invoked = callback(message)\n\
             val printed = println(message)\n\
         }",
    );
    let (stable_source, stable) = parsed(
        &mut sources,
        "p/stable.ko",
        "package p\nfun stable(): Unit {}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/stable.ko", stable_source, &stable),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");
    let uses_unit = source_unit(&names, uses_source);

    for text in ["callback(message)", "println(message)"] {
        let call = UnitExpressionId::new(uses_unit, expression_with_text(&sources, &uses, text));
        let contracts = ownership
            .call_argument_contracts_for(call)
            .collect::<Vec<_>>();
        assert_eq!(contracts.len(), 1, "{text}");
        assert_eq!(
            contracts[0].kind(),
            UnitCallArgumentOwnershipKind::SharedLoan,
            "{text}"
        );
        assert_eq!(contracts[0].parameter_span(), None, "{text}");
        assert_eq!(
            sources
                .slice(contracts[0].argument_span())
                .expect("argument span"),
            "message"
        );
    }
    assert_eq!(ownership.loans().len(), 2);
    assert!(
        ownership
            .loans()
            .iter()
            .all(|loan| loan.kind() == LoanKind::Shared)
    );
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn value_delivery_distinguishes_copy_move_and_temporary() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun deliver(own count: Int, own text: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(own count: Int, own text: String): Unit {\n\
             val first = p.deliver(count, text)\n\
             val second = p.deliver(7, \"ok\")\n\
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

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(
        ownership
            .value_deliveries()
            .iter()
            .map(|delivery| delivery.kind())
            .collect::<Vec<_>>(),
        [
            UnitValueDeliveryKind::Copy,
            UnitValueDeliveryKind::Move,
            UnitValueDeliveryKind::Temporary,
            UnitValueDeliveryKind::Temporary,
        ]
    );
    assert!(ownership.loans().is_empty());
}

#[test]
fn member_receiver_is_checked_as_the_zeroth_call_operand() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker(var count: Int) {\n\
             fun read(): Int = count\n\
             inout fun bump(): Unit { count = count + 1 }\n\
             own fun finish(): Unit {}\n\
         }\n\
         fun use(own first: Worker, own second: Worker): Unit {\n\
             val a = first.read()\n\
             val b = first.bump()\n\
             val c = second.finish()\n\
             val d = Worker(0).read()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.call_receiver_contracts().len(), 4);
    assert_eq!(ownership.receiver_facts().len(), 4);

    let unit = source_unit(&names, source);
    let first = symbol_named(&ownership, &names, unit, "first");
    let second = symbol_named(&ownership, &names, unit, "second");
    for (text, expected_kind, expected_root) in [
        (
            "first.read()",
            UnitReceiverOwnershipKind::SharedLoan,
            Some(first),
        ),
        (
            "first.bump()",
            UnitReceiverOwnershipKind::ExclusiveLoan,
            Some(first),
        ),
        (
            "second.finish()",
            UnitReceiverOwnershipKind::Move,
            Some(second),
        ),
        (
            "Worker(0).read()",
            UnitReceiverOwnershipKind::SharedLoan,
            None,
        ),
    ] {
        let call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text));
        let fact = ownership.receiver_fact(call).expect("receiver fact");
        assert_eq!(fact.kind(), expected_kind);
        assert!(fact.declaration_span().is_some());
        assert_eq!(sources.slice(fact.end_span()).expect("call span"), text);
        match (fact.source(), fact.target(), expected_root) {
            (
                UnitCallReceiverOrigin::Expression(origin),
                UnitReceiverOwnershipTarget::Place(place),
                Some(root),
            ) => {
                assert_eq!(origin.source_unit(), unit);
                assert_eq!(place.root(), root);
            }
            (
                UnitCallReceiverOrigin::Expression(origin),
                UnitReceiverOwnershipTarget::Temporary(temporary),
                None,
            ) => {
                assert_eq!(origin, *temporary);
            }
            actual => panic!("unexpected receiver fact: {actual:?}"),
        }
    }
    let temporary_call = UnitExpressionId::new(
        unit,
        expression_with_text(&sources, &parsed, "Worker(0).read()"),
    );
    let UnitReceiverOwnershipTarget::Temporary(temporary) = ownership
        .receiver_fact(temporary_call)
        .expect("temporary receiver fact")
        .target()
    else {
        panic!("Borrow temporary receiver must retain its expression identity");
    };
    assert!(ownership.drops().iter().any(|drop| {
        drop.point() == UnitDropPoint::CallReturn(temporary_call)
            && drop.target() == UnitDropTarget::Temporary(*temporary)
    }));
}

#[test]
fn stateless_object_borrow_receiver_has_no_runtime_drop_fact() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "object Registry { fun ping(): Int = 7 }\n\
         fun entry(): Int = Registry.ping()",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("stateless object ownership product");

    assert!(ownership.diagnostics().is_empty());
    let unit = source_unit(&names, source);
    let call = UnitExpressionId::new(
        unit,
        expression_with_text(&sources, &parsed, "Registry.ping()"),
    );
    let fact = ownership
        .receiver_fact(call)
        .expect("object Borrow receiver fact");
    assert_eq!(fact.kind(), UnitReceiverOwnershipKind::SharedLoan);
    assert!(matches!(
        fact.target(),
        UnitReceiverOwnershipTarget::Temporary(receiver) if *receiver == match fact.source() {
            UnitCallReceiverOrigin::Expression(receiver) => receiver,
            UnitCallReceiverOrigin::ImplicitThis(_) => panic!("object call uses an explicit receiver"),
        }
    ));
    assert!(
        ownership.drops().is_empty(),
        "a stateless object has no runtime owner to drop: {:?}",
        ownership.drops()
    );
}

#[test]
fn member_body_receiver_capability_controls_this_and_implicit_calls() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker(var count: Int) {\n\
             fun read(): Int = count\n\
             fun forwardRead(): Int = read()\n\
             inout fun bump(): Unit { count = count + 1 }\n\
             inout fun forwardBump(): Unit { val result = bump() }\n\
             own fun take(): Unit { val local = this }\n\
             own fun keep(): Unit {}\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let unit = source_unit(&names, source);
    for (text, expected) in [
        ("read()", UnitReceiverOwnershipKind::SharedLoan),
        ("bump()", UnitReceiverOwnershipKind::ExclusiveLoan),
    ] {
        let call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text));
        let fact = ownership
            .receiver_fact(call)
            .expect("implicit receiver fact");
        assert_eq!(fact.kind(), expected);
        assert!(matches!(
            (fact.source(), fact.target()),
            (
                UnitCallReceiverOrigin::ImplicitThis(source_owner),
                UnitReceiverOwnershipTarget::This(target_owner),
            ) if source_owner == *target_owner
        ));
    }
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), UnitDropTarget::This(_)))
            .count(),
        1,
        "only the unconsumed Value receiver is dropped as this",
    );
}

#[test]
fn implicit_value_receiver_remains_pending_until_call_commit() {
    for (exit, expected) in [("return", 1), ("error(\"abort\")", 0), ("true", 0)] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Host {{ own fun consume(text: String, own flag: Boolean): Unit {{}}\nown fun relay(own flag: Boolean): Unit {{ val done = consume(\"prefix\", if (flag) {{ {exit} }} else {{ true }}) }} }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &environment);
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        let call_text = format!("consume(\"prefix\", if (flag) {{ {exit} }} else {{ true }})");
        let call = UnitExpressionId::new(
            source_unit(&names, source),
            expression_with_text(&sources, &parsed, &call_text),
        );
        let UnitCallReceiverOrigin::ImplicitThis(relay) =
            owned.ownership().receiver_fact(call).unwrap().source()
        else {
            panic!("implicit receiver required")
        };
        let drops = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| {
                fact.target() == UnitDropTarget::This(relay)
                    && matches!(fact.point(), UnitDropPoint::ControlTransfer(_))
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), expected, "{exit}: {drops:?}");
        if exit == "return" {
            let ordered = owned
                .ownership()
                .drops()
                .iter()
                .filter(|fact| matches!(fact.point(), UnitDropPoint::ControlTransfer(_)))
                .map(|fact| fact.target())
                .collect::<Vec<_>>();
            let prefix = UnitExpressionId::new(
                source_unit(&names, source),
                expression_with_text(&sources, &parsed, "\"prefix\""),
            );
            assert_eq!(
                ordered,
                [
                    UnitDropTarget::Temporary(prefix),
                    UnitDropTarget::This(relay)
                ],
                "later argument owner must drop before the pending receiver"
            );
        }
    }
}

#[test]
fn static_self_value_receiver_publishes_conditional_drop_without_unconditional_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Finishable { own fun finish(): Int = 40 }\n\
         class Resource: Finishable {}\n\
         value class Counter(val item: Int): Finishable {}",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::This(_))),
        "StaticSelf must not become an unconditional receiver drop",
    );
    let [fact] = ownership.conditional_receiver_drops() else {
        panic!(
            "one StaticSelf receiver-drop obligation expected: {:?}",
            ownership.conditional_receiver_drops()
        );
    };
    let UnitDropPoint::ControlTransfer(body) = fact.point() else {
        panic!("expression-body receiver must drop on its return edge");
    };
    assert_eq!(sources.slice(fact.value_origin()).unwrap(), "finish");
    let Some(UnitTypeKind::StaticSelf(interface)) = typed.types().types().get(fact.receiver_type())
    else {
        panic!("conditional receiver type must be StaticSelf");
    };
    let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().types().get(*interface)
    else {
        panic!("StaticSelf must wrap the declaring interface type");
    };
    assert_eq!(fact.owner(), *declaration);
    assert_eq!(
        body,
        UnitExpressionId::new(
            source_unit(&names, source),
            expression_with_text(&sources, &parsed, "40"),
        ),
        "conditional drop point must name the exact default-body expression",
    );
}

#[test]
fn static_self_value_calls_publish_conditional_receiver_deliveries() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Parent { own fun inherited(): Int = 3 }\n\
         interface Chain: Parent {\n\
             fun read(): Int = 1\n\
             inout fun revise(): Unit {}\n\
             inout fun adjust(): Unit { val adjusted = revise() }\n\
             own fun endpoint(): Int = 2\n\
             own fun explicit(): Int {\n\
                 val observed = read()\n\
                 return this.endpoint()\n\
             }\n\
             own fun implicit(): Int = endpoint()\n\
             own fun inheritedExplicit(): Int = this.inherited()\n\
             own fun inheritedSuper(): Int = super<Parent>.inherited()\n\
         }\n\
         class Resource: Chain {}\n\
         value class Counter(val item: Int): Chain {}\n\
         fun exercise(): Unit {\n\
             val resource = Resource()\n\
             val moved = resource.explicit()\n\
             val counter = Counter(1)\n\
             val copied = counter.implicit()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
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
    let unit = source_unit(&names, source);
    let expected_calls = [
        ("this.endpoint()", Some("this")),
        ("endpoint()", Some("endpoint")),
        ("this.inherited()", Some("this")),
        ("super<Parent>.inherited()", Some("super<Parent>.inherited")),
    ]
    .map(|(text, delivery)| {
        (
            UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text)),
            delivery,
        )
    });
    assert_eq!(ownership.conditional_receiver_deliveries().len(), 4);
    for (call, delivery) in expected_calls {
        let fact = ownership
            .conditional_receiver_delivery(call)
            .expect("StaticSelf Value receiver delivery fact");
        let typed_call = typed
            .types()
            .calls()
            .iter()
            .find(|descriptor| descriptor.expression() == call)
            .expect("typed call descriptor");
        assert_eq!(fact.call(), call);
        assert_eq!(fact.target(), typed_call.target());
        assert!(matches!(
            typed.types().types().get(fact.receiver_type()),
            Some(UnitTypeKind::StaticSelf(_))
        ));
        assert!(ownership.conditional_receiver_drops().iter().any(|drop| {
            drop.owner() == fact.owner()
                && drop.receiver_type() == fact.receiver_type()
                && drop.value_origin() == fact.receiver_origin()
        }));
        assert!(ownership.receiver_fact(call).is_none());
        match fact.source() {
            UnitCallReceiverOrigin::Expression(receiver) => {
                assert_eq!(
                    sources.slice(fact.delivery_span()).unwrap(),
                    delivery.unwrap()
                );
                assert_eq!(receiver.source_unit(), unit);
            }
            UnitCallReceiverOrigin::ImplicitThis(owner) => {
                assert_eq!(owner, fact.owner());
                assert_eq!(
                    sources.slice(fact.delivery_span()).unwrap(),
                    delivery.unwrap()
                );
            }
        }
    }

    let read_call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, "read()"));
    assert_eq!(
        ownership
            .receiver_fact(read_call)
            .expect("Borrow receiver fact")
            .kind(),
        UnitReceiverOwnershipKind::SharedLoan,
    );
    let revise_call =
        UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, "revise()"));
    assert_eq!(
        ownership
            .receiver_fact(revise_call)
            .expect("Inout receiver fact")
            .kind(),
        UnitReceiverOwnershipKind::ExclusiveLoan,
    );
    assert!(
        ownership
            .conditional_receiver_delivery(revise_call)
            .is_none()
    );
    for (text, expected) in [
        ("resource.explicit()", UnitReceiverOwnershipKind::Move),
        ("counter.implicit()", UnitReceiverOwnershipKind::Copy),
    ] {
        let call = UnitExpressionId::new(unit, expression_with_text(&sources, &parsed, text));
        assert_eq!(
            ownership
                .receiver_fact(call)
                .expect("concrete Value receiver fact")
                .kind(),
            expected,
        );
        assert!(ownership.conditional_receiver_delivery(call).is_none());
    }
}

#[test]
fn ownership_error_clears_conditional_receiver_delivery_facts_atomically() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Finishable {\n\
             own fun finish(): Unit {}\n\
             own fun forward(): Unit { val result = finish() }\n\
         }\n\
         class Resource {}\n\
         fun invalid(own resource: Resource): Unit {\n\
             val first = resource\n\
             val second = resource\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership recovery product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131"]);
    assert!(ownership.conditional_receiver_deliveries().is_empty());
    assert!(ownership.conditional_receiver_drops().is_empty());
}

#[test]
fn conditional_receiver_drop_excludes_non_static_self_and_bodyless_receivers() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Actions {\n\
             fun inspect(): Unit {}\n\
             inout fun update(): Unit {}\n\
             own fun missing(): Unit\n\
             own fun finish(): Unit {}\n\
         }\n\
         class Resource { own fun consume(): Unit {} }\n\
         value class Counter(val item: Int) { own fun copy(): Unit {} }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let [conditional] = ownership.conditional_receiver_drops() else {
        panic!("only the bodyful StaticSelf Value receiver is conditional");
    };
    assert_eq!(sources.slice(conditional.value_origin()).unwrap(), "finish");
    let unconditional_origins = ownership
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), UnitDropTarget::This(_)))
        .map(|fact| sources.slice(fact.value_origin()).expect("receiver origin"))
        .collect::<Vec<_>>();
    assert_eq!(unconditional_origins, ["consume"]);
}

#[test]
fn ownership_error_clears_conditional_receiver_drop_facts_atomically() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Finishable { own fun finish(): Unit {} }\n\
         class Resource {}\n\
         fun invalid(own resource: Resource): Unit {\n\
             val first = resource\n\
             val second = resource\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership recovery product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131"]);
    assert!(ownership.conditional_receiver_drops().is_empty());
}

#[test]
fn receiver_loan_precedes_arguments_and_conflicts_with_overlapping_places() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker {\n\
             inout fun update(other: Worker): Unit {}\n\
             fun borrowThenTake(own other: Worker): Unit {}\n\
         }\n\
         fun first(own worker: Worker): Unit { val result = worker.update(worker) }\n\
         fun second(own worker: Worker): Unit { val result = worker.borrowThenTake(worker) }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0135", "L0135"]);
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn borrow_only_delegation_publishes_outer_and_field_ownership_plan() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "interface Readable { fun read(): Int }\n\
         class Reader: Readable { override fun read(): Int = 1 }\n\
         class Host(val reader: Reader): Readable by reader",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert!(ownership.deferred().is_empty());
    assert_eq!(ownership.delegations().len(), 1);
    let plan = &ownership.delegations()[0];
    assert_eq!(plan.target().source_unit(), source_unit(&names, source));
    assert_eq!(plan.forwarders().len(), 1);
    assert_eq!(
        sources
            .slice(plan.delegation_span())
            .expect("delegation span"),
        "by reader",
    );
}

#[test]
fn member_assignment_requires_a_var_field_and_inout_receiver() {
    for (assignment, expected_codes) in [
        ("fixed = 1", &["L0134"][..]),
        ("this.fixed = 2", &["L0134"][..]),
        ("(this).fixed = 2", &["L0134"][..]),
        ("count = 3", &[][..]),
        ("this.count = 4", &[][..]),
        ("(this).count = 4", &[][..]),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Worker(val fixed: Int, var count: Int) {{\n\
                 inout fun update(): Unit {{ {assignment} }}\n\
             }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");

        assert_eq!(diagnostic_codes(&ownership), expected_codes, "{assignment}");
        if !expected_codes.is_empty() {
            assert!(ownership.loans().is_empty());
            assert!(ownership.value_deliveries().is_empty());
        }
    }
}

#[test]
fn rejected_assignment_rolls_back_rhs_move_and_executable_facts() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Resource {}\n\
         fun identity(own item: Resource): Resource = item\n\
         class Holder(val fixed: Resource) {\n\
             inout fun reject(own replacement: Resource): Unit {\n\
                 val ignored: Unit = fixed = identity(replacement)\n\
                 val stillOwned: Resource = replacement\n\
             }\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0134"]);
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.rc_effects().is_empty());
    assert!(ownership.construction_plans().is_empty());
}

#[test]
fn divergent_rhs_still_checks_static_field_mutability() {
    for (method, expected_codes) in [
        (
            "inout fun reject(): Unit { fixed = error(\"stop\") }",
            &["L0134"][..],
        ),
        (
            "fun reject(): Unit { count = error(\"stop\") }",
            &["L0134"][..],
        ),
        (
            "inout fun accept(): Unit { count = error(\"stop\") }",
            &[][..],
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Holder(val fixed: Int, var count: Int) {{\n\
                 {method}\n\
             }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");

        assert_eq!(diagnostic_codes(&ownership), expected_codes, "{method}");
    }
}

#[test]
fn member_body_rejects_receiver_capability_escalation_and_use_after_move() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Worker(var count: Int) {\n\
             inout fun bump(): Unit { count = count + 1 }\n\
             own fun consume(): Unit {}\n\
             fun badWrite(): Unit { count = 1 }\n\
             fun badInout(): Unit { val result = bump() }\n\
             fun badValue(): Unit { val result = consume() }\n\
             own fun badAfterMove(): Int {\n\
                 val moved = this\n\
                 return count\n\
             }\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert_eq!(
        diagnostic_codes(&ownership),
        ["L0134", "L0134", "L0133", "L0131"]
    );
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn explicit_this_field_cannot_supply_inout_from_borrow_or_value_receiver() {
    for (mode, expected) in [
        ("borrow", vec!["L0134"]),
        ("own", vec!["L0134"]),
        ("inout", vec![]),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Cell(var n: Int) {{ inout fun set(): Unit {{ n = 1 }} }}\nclass Holder(val cell: Cell) {{ {mode} fun test(): Unit {{ this.cell.set() }} }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert_eq!(diagnostic_codes(&ownership), expected, "{mode}");
    }
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
fn borrowed_field_publishes_source_qualified_place_path() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Holder(var text: String)\n\
         fun read(message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Holder\n\
         fun use(holder: Holder): Unit { p.read(holder.text) }",
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

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.loans().len(), 1);
    let UnitLoanTarget::Place(place) = ownership.loans()[0].target() else {
        panic!("field borrow must establish a place loan");
    };
    let consumer_unit = source_unit(&names, consumer_source);
    assert_eq!(
        place.root(),
        symbol_named(&ownership, &names, consumer_unit, "holder")
    );
    assert_eq!(place.fields().len(), 1);
    assert_eq!(
        place.fields()[0].source_unit(),
        source_unit(&names, provider_source)
    );
    assert!(
        names.names().source_units()[place.fields()[0].source_unit().index()]
            .resolution()
            .symbols()
            .get(place.fields()[0].symbol().index())
            .is_some_and(|symbol| symbol.name() == "text")
    );
}

#[test]
fn moved_value_use_reports_cross_file_parameter_and_clears_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun consume(own payload: String): Unit {}\n\
         fun read(message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(own message: String): Unit {\n\
             val consumed = p.consume(message)\n\
             val read = p.read(message)\n\
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
        ownership
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0131"]
    );
    let diagnostic = &ownership.diagnostics()[0];
    assert_eq!(
        sources
            .slice(diagnostic.primary_span())
            .expect("primary span"),
        "message"
    );
    let labels = diagnostic
        .details()
        .iter()
        .filter_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some((
                label.message().to_owned(),
                sources.slice(label.span()).expect("label span").to_owned(),
            )),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .collect::<Vec<_>>();
    assert!(
        labels
            .iter()
            .any(|(message, _)| message == "value was moved here")
    );
    assert!(labels.iter().any(|(message, text)| {
        message == "selected parameter declared here" && text.contains("message")
    }));
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn moving_borrowed_binding_to_cross_file_value_parameter_is_rejected() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun consume(own payload: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun use(message: String): Unit { p.consume(message) }",
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
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0133");
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert_eq!(
        ownership.diagnostics()[0]
            .details()
            .iter()
            .filter(|detail| matches!(detail, DiagnosticDetail::Label(_)))
            .count(),
        2
    );
}

#[test]
fn shared_and_exclusive_alias_in_one_cross_file_call_conflict() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun alias(sharedValue: String, inout mutableValue: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun use(inout message: String): Unit { p.alias(message, &message) }",
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
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0135");
    assert!(ownership.diagnostics()[0].details().iter().any(|detail| {
        matches!(
            detail,
            DiagnosticDetail::Label(label) if label.message() == "conflicting loan starts here"
        )
    }));
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn shared_loan_allows_copyable_value_delivery_from_same_place() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun duplicate(sharedNumber: Int, own copiedNumber: Int): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun use(own number: Int): Unit { p.duplicate(number, number) }",
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

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.loans().len(), 1);
    assert_eq!(ownership.value_deliveries().len(), 1);
    assert_eq!(
        ownership.value_deliveries()[0].kind(),
        UnitValueDeliveryKind::Copy
    );
}

#[test]
fn immutable_inout_reports_ampersand_and_clears_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun mutate(inout mutableMessage: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun use(message: String): Unit { p.mutate(&message) }",
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
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0134");
    assert_eq!(
        sources
            .slice(ownership.diagnostics()[0].primary_span())
            .expect("primary span"),
        "&"
    );
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn borrowed_move_wins_over_earlier_shared_loan_conflict() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun conflict(sharedMessage: String, own movedMessage: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun use(message: String): Unit { p.conflict(message, message) }",
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
        ownership
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0133"]
    );
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
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
fn element_and_rc_payload_borrows_publish_owner_qualified_places() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun read(number: Int): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(numbers: List<Int>, owner: Rc<Int>): Unit {\n\
             val element = p.read(numbers[0])\n\
             val payload = p.read(owner.value)\n\
             val retained = owner.share()\n\
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

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.loans().len(), 2);
    assert_eq!(ownership.rc_effects().len(), 2);
    assert_eq!(
        ownership
            .rc_effects()
            .iter()
            .map(|effect| effect.kind())
            .collect::<Vec<_>>(),
        [
            RcOwnershipEffectKind::BorrowPayload,
            RcOwnershipEffectKind::Retain,
        ]
    );
    let consumer_unit = source_unit(&names, consumer_source);
    let UnitLoanTarget::Place(element) = ownership.loans()[0].target() else {
        panic!("element borrow must use a place");
    };
    assert_eq!(
        element.root(),
        symbol_named(&ownership, &names, consumer_unit, "numbers")
    );
    assert!(element.element().is_some());
    let UnitLoanTarget::Place(payload_owner) = ownership.loans()[1].target() else {
        panic!("Rc payload borrow must retain the owner place");
    };
    assert_eq!(
        payload_owner.root(),
        symbol_named(&ownership, &names, consumer_unit, "owner")
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
    .expect("reversed unit ownership product");
    assert_eq!(ownership.rc_effects(), reversed.rc_effects());
}

#[test]
fn when_entry_bodies_do_not_observe_later_condition_moves() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun take(own message: String): Boolean = true\n\
         fun read(message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(own message: String, flag: Boolean): Unit {\n\
             when {\n\
                 flag -> { p.read(message) }\n\
                 p.take(message) -> {}\n\
                 else -> {}\n\
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
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.loans().len(), 1);
    assert_eq!(ownership.value_deliveries().len(), 1);
    assert_eq!(
        ownership.value_deliveries()[0].kind(),
        UnitValueDeliveryKind::Move
    );
}

#[test]
fn local_var_and_temporary_rc_payload_are_valid_loan_targets() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun mutate(inout message: String): Unit {}\n\
         fun read(number: Int): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(own input: String): Unit {\n\
             var local = input\n\
             val changed = p.mutate(&local)\n\
             val read = p.read(Rc(1).value)\n\
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

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.loans().len(), 2);
    assert!(matches!(
        ownership.loans()[1].target(),
        UnitLoanTarget::Temporary(_)
    ));
}

#[test]
fn move_only_container_element_value_delivery_reports_l0136() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun consume(own resource: Resource): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         fun use(own resources: MutableList<Resource>): Unit { p.consume(resources[0]) }",
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
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0136");
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn element_move_conflict_precedes_container_element_move_diagnostic() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun conflict(first: Resource, own second: Resource): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         fun use(own resources: MutableList<Resource>): Unit {\n\
             p.conflict(resources[0], resources[0])\n\
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
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0135");
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
}

#[test]
fn temporary_rc_payload_publishes_borrow_effect_and_temporary_loan() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun read(number: Int): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun use(): Unit { val read = p.read(Rc(1).value) }",
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

    assert!(ownership.diagnostics().is_empty());
    assert_eq!(ownership.rc_effects().len(), 1);
    assert_eq!(
        ownership.rc_effects()[0].kind(),
        RcOwnershipEffectKind::BorrowPayload
    );
    assert!(matches!(
        ownership.loans()[0].target(),
        UnitLoanTarget::Temporary(_)
    ));
}

#[test]
fn rc_move_errors_clear_all_executable_unit_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun consume(own resource: Resource): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         fun extract(own owner: Rc<Resource>): Unit { p.consume(owner.value) }\n\
         fun reuse(own owner: Rc<Int>): Unit {\n\
             val moved = owner\n\
             val retained = owner.share()\n\
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
        ownership
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0132", "L0131"]
    );
    assert!(ownership.diagnostics()[0].details().iter().any(|detail| {
        matches!(
            detail,
            DiagnosticDetail::Label(label)
                if label.message() == "selected parameter declared here"
        )
    }));
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.rc_effects().is_empty());
}

#[test]
fn source_constructions_publish_ordered_deliveries_root_kinds_and_stable_identity() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         class Holder(val first: Resource, val count: Int, val second: Resource)\n\
         value class Token(val resource: Resource)\n\
         value class Count(val count: Int)\n\
         enum class Choice { Item(resource: Resource), Empty }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.Holder\n\
         import p.Token\n\
         import p.Count\n\
         import p.Choice\n\
         fun build(own first: Resource, own second: Resource): Unit {\n\
             val holder = Holder(second = second, count = 1, first = first)\n\
             val boxed = Box(Token(Resource()))\n\
             val shared = Rc(Resource())\n\
             val item = Choice.Item(Resource())\n\
             val count = Count(2)\n\
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

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let holder_expression = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(
            &sources,
            &consumer,
            "Holder(second = second, count = 1, first = first)",
        ),
    );
    let holder = ownership
        .construction_plans()
        .iter()
        .find(|plan| plan.construction() == holder_expression)
        .expect("Holder ownership plan");
    assert_eq!(
        holder
            .deliveries()
            .iter()
            .map(|delivery| delivery.parameter_index())
            .collect::<Vec<_>>(),
        [2, 1, 0]
    );
    assert_eq!(
        holder
            .deliveries()
            .iter()
            .map(|delivery| delivery.evaluation_index())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        holder
            .deliveries()
            .iter()
            .map(|delivery| delivery.kind())
            .collect::<Vec<_>>(),
        [
            ConstructionDeliveryKind::Move,
            ConstructionDeliveryKind::DeliverTemporary,
            ConstructionDeliveryKind::Move,
        ]
    );
    assert_eq!(
        holder.root_obligation().expect("Holder root").kind(),
        ConstructionRootKind::HeapOwner
    );

    for (text, expected) in [
        ("Box(Token(Resource()))", ConstructionRootKind::HeapOwner),
        ("Rc(Resource())", ConstructionRootKind::SharedOwner),
        ("Token(Resource())", ConstructionRootKind::Inline),
        ("Choice.Item(Resource())", ConstructionRootKind::Inline),
    ] {
        let expression = UnitExpressionId::new(
            consumer_unit,
            expression_with_text(&sources, &consumer, text),
        );
        let plan = ownership
            .construction_plans()
            .iter()
            .find(|plan| plan.construction() == expression)
            .expect("construction plan");
        assert_eq!(
            plan.root_obligation().expect("MoveOnly root").kind(),
            expected
        );
    }
    let count_expression = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "Count(2)"),
    );
    let count = ownership
        .construction_plans()
        .iter()
        .find(|plan| plan.construction() == count_expression)
        .expect("Copyable value construction plan");
    assert!(count.root_obligation().is_none());
    assert_eq!(
        count.deliveries()[0].kind(),
        ConstructionDeliveryKind::DeliverTemporary
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
    assert_eq!(
        ownership.construction_plans(),
        reversed.construction_plans()
    );
}

#[test]
fn construction_use_after_move_labels_cross_file_field_and_clears_plans() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         class Holder(val first: Resource, val second: Resource)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.Holder\n\
         fun invalid(own resource: Resource): Unit {\n\
             val holder = Holder(resource, resource)\n\
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
    assert!(ownership.diagnostics()[0].details().iter().any(|detail| {
        matches!(
            detail,
            DiagnosticDetail::Label(label)
                if label.message() == "selected parameter declared here"
        )
    }));
    assert!(ownership.construction_plans().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.rc_effects().is_empty());
}

#[test]
fn construction_move_conflicts_with_earlier_call_loan_and_labels_cross_file_field() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         class Holder(val resource: Resource)\n\
         fun outer(first: Resource, own holder: Holder): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.Holder\n\
         fun invalid(own resource: Resource): Unit {\n\
             val result = p.outer(resource, Holder(resource))\n\
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
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0135");
    assert!(ownership.diagnostics()[0].details().iter().any(|detail| {
        matches!(
            detail,
            DiagnosticDetail::Label(label)
                if label.message() == "selected parameter declared here"
        )
    }));
    assert!(ownership.construction_plans().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.rc_effects().is_empty());
}

#[test]
fn construction_rejects_move_only_container_element_with_field_label() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         class Holder(val resource: Resource)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.Holder\n\
         fun invalid(resources: List<Resource>): Unit {\n\
             val holder = Holder(resources[0])\n\
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
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0136");
    assert!(ownership.diagnostics()[0].details().iter().any(|detail| {
        matches!(
            detail,
            DiagnosticDetail::Label(label)
                if label.message() == "selected parameter declared here"
        )
    }));
    assert!(ownership.construction_plans().is_empty());
}

#[test]
fn nothing_operand_publishes_only_the_completed_construction_prefix() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         class Holder(val first: Resource, val second: Resource)\n\
         fun stop(): Nothing = error(\"stop\")",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.Holder\n\
         fun build(): Unit { val holder = Holder(Resource(), p.stop()) }",
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

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let expression = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "Holder(Resource(), p.stop())"),
    );
    let holder = ownership
        .construction_plans()
        .iter()
        .find(|plan| plan.construction() == expression)
        .expect("partial Holder plan");
    assert_eq!(holder.deliveries().len(), 1);
    assert!(holder.root_obligation().is_none());
    assert!(holder.terminating_operand().is_some());
}

#[test]
fn container_constructions_publish_value_deliveries_borrow_loans_and_stable_identity() {
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
         fun build(own resource: Resource, number: Int, initializer: (Int) -> Int): Unit {\n\
             val resources = listOf(resource)\n\
             val numbers = listOf(number, number)\n\
             val generated = List<Int>(borrow number, initializer)\n\
             val empty = MutableList<Resource>()\n\
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

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let list_resource = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "listOf(resource)"),
    );
    let list_number = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "listOf(number, number)"),
    );
    let runtime = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "List<Int>(borrow number, initializer)"),
    );
    let empty = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "MutableList<Resource>()"),
    );

    assert_eq!(
        ownership
            .call_argument_contracts_for(list_resource)
            .map(|contract| contract.kind())
            .collect::<Vec<_>>(),
        [UnitCallArgumentOwnershipKind::Value]
    );
    assert_eq!(
        ownership
            .call_argument_contracts_for(list_number)
            .map(|contract| contract.kind())
            .collect::<Vec<_>>(),
        [
            UnitCallArgumentOwnershipKind::Value,
            UnitCallArgumentOwnershipKind::Value,
        ]
    );
    assert_eq!(
        ownership
            .call_argument_contracts_for(runtime)
            .map(|contract| contract.kind())
            .collect::<Vec<_>>(),
        [
            UnitCallArgumentOwnershipKind::SharedLoan,
            UnitCallArgumentOwnershipKind::SharedLoan,
        ]
    );
    assert_eq!(ownership.call_argument_contracts_for(empty).count(), 0);
    assert_eq!(
        ownership
            .value_deliveries()
            .iter()
            .map(|delivery| (delivery.call(), delivery.kind()))
            .collect::<Vec<_>>(),
        [
            (list_resource, UnitValueDeliveryKind::Move),
            (list_number, UnitValueDeliveryKind::Copy),
            (list_number, UnitValueDeliveryKind::Copy),
        ]
    );
    assert_eq!(ownership.loans().len(), 2);
    assert!(ownership.loans().iter().all(|loan| {
        loan.call() == runtime
            && loan.kind() == LoanKind::Shared
            && matches!(loan.target(), UnitLoanTarget::Place(_))
    }));

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
    assert_eq!(
        ownership.call_argument_contracts(),
        reversed.call_argument_contracts()
    );
    assert_eq!(ownership.loans(), reversed.loans());
    assert_eq!(ownership.value_deliveries(), reversed.value_deliveries());
}

#[test]
fn runtime_container_nothing_operands_keep_expected_contracts_and_stop_effects() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun stop(): Nothing = error(\"stop\")",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun first(own resource: String, initializer: (Int) -> Int): Unit {\n\
             val generated = List<Int>(p.stop(), initializer)\n\
             val unreachable = listOf(resource)\n\
         }\n\
         fun second(own resource: String, number: Int): Unit {\n\
             val generated = List<Int>(number, p.stop())\n\
             val unreachable = listOf(resource)\n\
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
            .expect("bottom operands remain valid ownership input");

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let first = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "List<Int>(p.stop(), initializer)"),
    );
    let second = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "List<Int>(number, p.stop())"),
    );
    let first_contracts = ownership
        .call_argument_contracts_for(first)
        .collect::<Vec<_>>();
    let second_contracts = ownership
        .call_argument_contracts_for(second)
        .collect::<Vec<_>>();
    assert_eq!(first_contracts.len(), 2);
    assert_eq!(second_contracts.len(), 2);
    assert_eq!(
        typed
            .types()
            .types()
            .get(first_contracts[0].parameter_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert!(matches!(
        typed
            .types()
            .types()
            .get(second_contracts[1].parameter_type()),
        Some(UnitTypeKind::Function { .. })
    ));
    assert_eq!(ownership.loans_ending_at(first).count(), 0);
    assert_eq!(ownership.loans_ending_at(second).count(), 1);

    let calls = consumer
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, expression)| {
            sources
                .slice(expression.span())
                .is_ok_and(|actual| actual == "listOf(resource)")
                .then_some(UnitExpressionId::new(consumer_unit, id))
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| {
        ownership
            .value_deliveries()
            .iter()
            .all(|delivery| delivery.call() != *call)
    }));
}

#[test]
fn list_form_move_reports_reuse_and_clears_executable_facts() {
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
             val resources = listOf(resource)\n\
             val reused = resource\n\
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
    assert_eq!(ownership.call_argument_contracts().len(), 1);
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.rc_effects().is_empty());
    assert!(ownership.construction_plans().is_empty());
    assert!(ownership.drops().is_empty());
    assert!(ownership.clone().validate().is_err());
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
fn unit_asap_drop_facts_cover_return_temporary_replacement_and_control_edges() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         class Holder(var item: Resource) {\n\
         inout fun replace(own replacement: Resource): Unit {\n\
                 val result = (this.item = replacement)\n\
             }\n\
             inout fun stop(): Unit {\n\
                 val result = (item = error(\"stop\"))\n\
             }\n\
         }\n\
         class CopyHolder(var item: Int) {\n\
             inout fun replace(own replacement: Int): Unit {\n\
                 val result = (item = replacement)\n\
             }\n\
         }\n\
         fun create(): Resource\n\
         fun inspect(item: Resource): Unit {}\n\
         fun replaceOther(inout holder: Holder, own replacement: Resource): Unit {\n\
             val result = (holder.item = replacement)\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Resource\n\
         import p.create\n\
         import p.inspect\n\
         fun drops(flag: Boolean, own unusedParameter: Resource, own branchOwner: Resource): Unit {\n\
             val unused = create()\n\
             val used = create()\n\
             val first = inspect(used)\n\
             var replaced = create()\n\
             { replaced = create() }\n\
             val temporary = inspect(create())\n\
             if (flag) {\n\
                 val branchRead = inspect(branchOwner)\n\
                 val early = create()\n\
                 if (flag) { return }\n\
                 val after = inspect(early)\n\
             } else {\n\
                 val branch = create()\n\
             }\n\
             while (flag) {\n\
                 val loopRead = inspect(replaced)\n\
                 break\n\
             }\n\
         }\n\
         fun returned(own result: Resource, own spare: Resource): Resource {\n\
             return result\n\
         }\n\
         fun captured(own item: Resource): Unit {\n\
             val closure: move () -> Unit = move { val read = inspect(item) }\n\
         }\n\
         fun stringDrops(own left: String, own right: String): Boolean {\n\
             val joined = left + \"!\"\n\
             return joined == right\n\
         }\n\
         fun elementDrop(own items: MutableList<Resource>, own replacement: Resource): Unit {\n\
             val result = (items[0] = replacement)\n\
         }\n\
         fun divergentAssignment(): Unit {\n\
             var target = create()\n\
             val result = (target = error(\"root-stop\"))\n\
             val unreachable = create()\n\
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
    assert!(ownership.deferred().is_empty());
    let provider_unit = source_unit(&names, provider_source);
    let consumer_unit = source_unit(&names, consumer_source);
    let field_assignments = [
        UnitExpressionId::new(
            provider_unit,
            expression_with_text(&sources, &provider, "this.item = replacement"),
        ),
        UnitExpressionId::new(
            provider_unit,
            expression_with_text(&sources, &provider, "holder.item = replacement"),
        ),
    ];
    let mut replaced_field = None;
    for field_assignment in field_assignments {
        let field_descriptor = typed
            .types()
            .assignment(field_assignment)
            .expect("MoveOnly field assignment descriptor");
        let projection = typed
            .types()
            .aggregate_projection(field_descriptor.target())
            .expect("MoveOnly field projection");
        if let Some(expected) = replaced_field {
            assert_eq!(projection.field(), expected, "same Holder field identity");
        } else {
            replaced_field = Some(projection.field());
        }
        let facts = ownership
            .drops()
            .iter()
            .filter(|fact| {
                matches!(
                    fact.point(),
                    UnitDropPoint::BeforeReplacement(expression)
                        if expression == field_assignment
                )
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            facts.as_slice(),
            [fact]
                if matches!(
                    fact.target(),
                    UnitDropTarget::ReplacedField {
                        assignment,
                        field,
                    } if assignment == field_assignment && field == projection.field()
                ) && sources.slice(fact.value_origin()).unwrap().ends_with("item")
        ));
    }
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::BeforeReplacement(_)))
            .count(),
        2,
        "Copyable and non-fallthrough field assignments must not publish old-field drop facts"
    );
    let unreachable_drops = ownership
        .drops()
        .iter()
        .filter(|fact| {
            matches!(fact.target(), UnitDropTarget::Named(_))
                && sources.slice(fact.value_origin()).unwrap() == "unreachable"
        })
        .collect::<Vec<_>>();
    assert!(
        unreachable_drops.is_empty(),
        "an aborting replacement RHS must stop planning unreachable statements: {unreachable_drops:?}"
    );
    let target_drops = ownership
        .drops()
        .iter()
        .filter(|fact| {
            matches!(fact.target(), UnitDropTarget::Named(_))
                && sources.slice(fact.value_origin()).unwrap() == "target"
        })
        .collect::<Vec<_>>();
    assert_eq!(
        target_drops.len(),
        1,
        "ASAP may drop the unread target before abort, but must not add a second function-exit drop: {target_drops:?}"
    );
    let named_origins = ownership
        .drops()
        .iter()
        .filter_map(|fact| match fact.target() {
            UnitDropTarget::Named(_) => Some(sources.slice(fact.value_origin()).unwrap()),
            UnitDropTarget::This(_)
            | UnitDropTarget::Temporary(_)
            | UnitDropTarget::ReplacedElement(_)
            | UnitDropTarget::ReplacedField { .. }
            | UnitDropTarget::Captured { .. } => None,
        })
        .collect::<Vec<_>>();
    for expected in [
        "unusedParameter",
        "unused",
        "used",
        "replaced",
        "early",
        "branch",
        "spare",
        "closure",
    ] {
        assert!(
            named_origins.contains(&expected),
            "missing {expected}: {named_origins:?}"
        );
    }
    assert!(
        !named_origins.contains(&"result"),
        "returned owner must transfer instead of drop: {named_origins:?}"
    );
    for predicate in [
        ownership.drops().iter().any(|fact| {
            matches!(
                fact.point(),
                UnitDropPoint::FunctionEntry(item) if item.source_unit() == consumer_unit
            )
        }),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::AfterStatement(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::AfterBinaryOperands(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::CallReturn(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::ControlTransfer(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::BranchExit { .. })),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::LoopExit(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), UnitDropPoint::AfterReplacement(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::Temporary(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::ReplacedElement(_))),
        ownership
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::Captured { .. })),
    ] {
        assert!(predicate, "{:?}", ownership.drops());
    }
    let validated = ownership
        .clone()
        .validate()
        .expect("complete ownership product validates");
    assert!(validated.ownership().is_compatible_with(&typed));

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
    .expect("reversed unit ownership product");
    assert_eq!(ownership.drops(), reversed.drops());
    assert!(reversed.validate().is_ok());
}

#[test]
fn lambda_tail_consumes_result_and_drops_only_body_owned_inputs() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun make(): String = \"made\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         val topAction: move () -> String = move {\n\
             val result = \"owned\"\n\
             return result\n\
         }\n\
         fun entry(): Unit {\n\
             val action: move () -> String = move { \"left\" + p.make() }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let tail = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "\"left\" + p.make()"),
    );
    let left = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "\"left\""),
    );
    let call = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "p.make()"),
    );
    let operand_drops = ownership
        .drops()
        .iter()
        .filter_map(|fact| {
            (fact.point() == UnitDropPoint::AfterBinaryOperands(tail)).then_some(fact.target())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        operand_drops,
        [
            UnitDropTarget::Temporary(call),
            UnitDropTarget::Temporary(left)
        ],
        "lambda tail operands drop in reverse evaluation order"
    );
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|fact| fact.target() == UnitDropTarget::Temporary(tail)),
        "implicit lambda result transfers to the caller instead of being dropped"
    );
    let result = names.names().source_units()[consumer_unit.index()]
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "result")
        .expect("lambda body local exists")
        .id();
    assert!(
        !ownership.drops().iter().any(|fact| {
            matches!(
                fact.target(),
                UnitDropTarget::Named(target)
                    if target.source_unit() == consumer_unit && target.symbol() == result
            )
        }),
        "a body-local owner returned from a top-level initializer lambda remains live until transfer"
    );
    ownership
        .clone()
        .validate()
        .expect("complete lambda drop facts validate");
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
    .expect("reversed unit ownership product");
    assert_eq!(ownership.drops(), reversed.drops());
}

#[test]
fn lambda_value_parameters_publish_entry_read_and_transfer_drop_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun inspect(message: String): Unit {}\n\
         fun consume(own message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val unused: move (own String, own String) -> String = move { first, second -> \"unused-a\" + \"unused-b\" }\n\
             val read: move (own String) -> Unit = move { observed -> p.inspect(observed) }\n\
             val consumed: move (own String) -> Unit = move { delivered -> p.consume(delivered) }\n\
             val implicitAction: move (own String) -> String = move { transferred -> transferred }\n\
             val explicitAction: move (own String) -> String = move { explicitValue -> return explicitValue }\n\
             val borrowed: move (borrow String) -> String = move { item -> \"borrow-a\" + \"borrow-b\" }\n\
             val copied: move (own Int) -> String = move { item -> \"copy-a\" + \"copy-b\" }\n\
             val implicitUnused: move (own String) -> String = move { \"implicit-a\" + \"implicit-b\" }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let source_unit = source_unit(&names, consumer_source);
    let unused_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { first, second -> \"unused-a\" + \"unused-b\" }",
        ),
    );
    let read_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { observed -> p.inspect(observed) }",
        ),
    );
    let returned_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(&sources, &consumer, "move { transferred -> transferred }"),
    );
    let consumed_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { delivered -> p.consume(delivered) }",
        ),
    );
    let explicit_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { explicitValue -> return explicitValue }",
        ),
    );
    let borrowed_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { item -> \"borrow-a\" + \"borrow-b\" }",
        ),
    );
    let copied_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { item -> \"copy-a\" + \"copy-b\" }",
        ),
    );
    let implicit_unused_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { \"implicit-a\" + \"implicit-b\" }",
        ),
    );
    let first = symbol_named(&ownership, &names, source_unit, "first");
    let second = symbol_named(&ownership, &names, source_unit, "second");
    let observed = symbol_named(&ownership, &names, source_unit, "observed");
    let entry_targets = ownership
        .drops()
        .iter()
        .filter_map(|fact| {
            (fact.point() == UnitDropPoint::LambdaEntry(unused_lambda)).then_some(fact.target())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        entry_targets,
        [UnitDropTarget::Named(second), UnitDropTarget::Named(first)],
        "unused MoveOnly Value parameters drop at lambda entry in reverse declaration order"
    );
    let implicit_it = symbol_named(&ownership, &names, source_unit, "it");
    assert!(ownership.drops().iter().any(|fact| {
        fact.point() == UnitDropPoint::LambdaEntry(implicit_unused_lambda)
            && fact.target() == UnitDropTarget::Named(implicit_it)
    }));
    assert!(
        ownership.drops().iter().any(|fact| {
            fact.target() == UnitDropTarget::Named(observed)
                && matches!(fact.point(), UnitDropPoint::CallReturn(_))
        }),
        "a borrowed read drops the owned parameter after the synchronous call returns"
    );
    let transferred_parameter_drops = ownership
        .drops()
        .iter()
        .filter_map(|fact| {
            let origin = sources.slice(fact.value_origin()).ok()?;
            (origin == "delivered" || origin == "transferred" || origin == "explicitValue")
                .then_some((origin, fact))
        })
        .collect::<Vec<_>>();
    assert!(
        transferred_parameter_drops.is_empty(),
        "Value calls and lambda returns transfer their MoveOnly Value parameters without dropping them: {transferred_parameter_drops:?}"
    );
    for lambda in [
        read_lambda,
        consumed_lambda,
        returned_lambda,
        explicit_lambda,
        borrowed_lambda,
        copied_lambda,
    ] {
        assert!(
            !ownership
                .drops()
                .iter()
                .any(|fact| fact.point() == UnitDropPoint::LambdaEntry(lambda)),
            "only unused MoveOnly Value parameters produce lambda-entry drops"
        );
    }
    let unused_tail = UnitExpressionId::new(
        source_unit,
        expression_with_text(&sources, &consumer, "\"unused-a\" + \"unused-b\""),
    );
    assert!(
        ownership
            .drops()
            .iter()
            .any(|fact| fact.point() == UnitDropPoint::AfterBinaryOperands(unused_tail)),
        "MoveOnly Value parameters no longer defer lambda body drop planning"
    );
    ownership
        .clone()
        .validate()
        .expect("lambda parameter drop facts validate");
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
    .expect("reversed unit ownership product");
    assert_eq!(ownership.drops(), reversed.drops());
    reversed
        .validate()
        .expect("reversed lambda parameter drop facts validate");
}

#[test]
fn move_only_control_results_transfer_branch_owners_and_drop_only_inputs() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun make(): String = \"made\"\n\
         fun inspect(message: String): Unit {}\n\
         fun wrap(message: String): String = \"wrapped\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun selected(flag: Boolean): String {\n\
             val leftOwner = \"left-owner\"\n\
             val rightOwner = \"right-owner\"\n\
             val result = if (flag) { leftOwner } else { rightOwner }\n\
             return result\n\
         }\n\
         fun nested(flag: Boolean): String = when (flag) {\n\
             true -> if (flag) { \"nested-a\" + p.make() } else { \"nested-b\" + \"nested-c\" }\n\
             false -> \"when-a\" + \"when-b\"\n\
         }\n\
         fun initialized(flag: Boolean): String {\n\
             val result = when (flag) {\n\
                 true -> \"init-a\" + \"init-b\"\n\
                 false -> \"init-c\" + \"init-d\"\n\
             }\n\
             return result\n\
         }\n\
         fun diverging(flag: Boolean): String {\n\
             val result = if (flag) { \"normal-a\" + \"normal-b\" } else { return \"early\" }\n\
             return result\n\
         }\n\
         fun aborting(flag: Boolean): String {\n\
             val result = if (flag) { \"survive-a\" + \"survive-b\" } else { error(\"stop\") }\n\
             return result\n\
         }\n\
         fun nestedAborting(flag: Boolean): String {\n\
             val result = if (flag) { \"nested-live-a\" + \"nested-live-b\" } else { p.wrap(error(\"nested-stop\")) }\n\
             return result\n\
         }\n\
         fun borrowed(flag: Boolean): Unit {\n\
             val borrowLeft = \"borrow-left\"\n\
             val borrowRight = \"borrow-right\"\n\
             val seen = p.inspect(if (flag) { borrowLeft } else { borrowRight })\n\
         }\n\
         fun discarded(flag: Boolean): Unit {\n\
             val discardLeft = \"discard-left\"\n\
             val discardRight = \"discard-right\"\n\
             if (flag) { discardLeft } else { discardRight }\n\
         }\n\
         fun entry(): Unit {\n\
             val action: move (borrow Boolean) -> String = move { flag ->\n\
                 when (flag) {\n\
                     true -> \"lambda-a\" + \"lambda-b\"\n\
                     false -> \"lambda-c\" + \"lambda-d\"\n\
                 }\n\
             }\n\
             val sibling: move () -> String = move { \"sibling-a\" + \"sibling-b\" }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let expression = |text| {
        UnitExpressionId::new(
            consumer_unit,
            expression_with_text(&sources, &consumer, text),
        )
    };
    let has_operand_drop = |text| {
        let expression = expression(text);
        ownership
            .drops()
            .iter()
            .any(|fact| fact.point() == UnitDropPoint::AfterBinaryOperands(expression))
    };
    for transferred in [
        "\"nested-a\" + p.make()",
        "\"nested-b\" + \"nested-c\"",
        "\"when-a\" + \"when-b\"",
        "\"init-a\" + \"init-b\"",
        "\"init-c\" + \"init-d\"",
        "\"normal-a\" + \"normal-b\"",
        "\"survive-a\" + \"survive-b\"",
        "\"nested-live-a\" + \"nested-live-b\"",
        "\"lambda-a\" + \"lambda-b\"",
        "\"lambda-c\" + \"lambda-d\"",
        "\"sibling-a\" + \"sibling-b\"",
    ] {
        assert!(
            has_operand_drop(transferred),
            "a consumed control tail drops only its owned composite inputs"
        );
        let result = expression(transferred);
        assert!(
            !ownership
                .drops()
                .iter()
                .any(|fact| fact.target() == UnitDropTarget::Temporary(result)),
            "the control tail result {transferred} transfers instead of being dropped: {:?}",
            ownership.drops()
        );
    }
    let nested = expression("\"nested-a\" + p.make()");
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter_map(|fact| {
                (fact.point() == UnitDropPoint::AfterBinaryOperands(nested))
                    .then_some(fact.target())
            })
            .collect::<Vec<_>>(),
        [
            UnitDropTarget::Temporary(expression("p.make()")),
            UnitDropTarget::Temporary(expression("\"nested-a\""))
        ],
        "control-tail composite operands drop in reverse evaluation order"
    );
    let early = expression("\"early\"");
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|fact| fact.target() == UnitDropTarget::Temporary(early)),
        "an explicit return branch transfers its value before control cleanup"
    );
    let stop = expression("error(\"stop\")");
    assert!(
        !ownership.drops().iter().any(|fact| {
            matches!(
                fact.point(),
                UnitDropPoint::BranchExit {
                    control,
                    branch: 1
                } if control == expression("if (flag) { \"survive-a\" + \"survive-b\" } else { error(\"stop\") }")
            ) || fact.target() == UnitDropTarget::Temporary(stop)
        }),
        "a Nothing branch does not publish a normal branch exit or result owner"
    );
    let nested_stop = expression("p.wrap(error(\"nested-stop\"))");
    assert!(
        !ownership.drops().iter().any(|fact| {
            matches!(
                fact.point(),
                UnitDropPoint::BranchExit {
                    control,
                    branch: 1
                } if control == expression(
                    "if (flag) { \"nested-live-a\" + \"nested-live-b\" } else { p.wrap(error(\"nested-stop\")) }"
                )
            ) || fact.target() == UnitDropTarget::Temporary(nested_stop)
        }),
        "a nested Nothing argument prevents its enclosing call from publishing a branch exit"
    );
    let named_drops = |name| {
        ownership
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), UnitDropTarget::Named(_))
                    && sources
                        .slice(fact.value_origin())
                        .is_ok_and(|origin| origin == name)
            })
            .map(|fact| fact.point())
            .collect::<Vec<_>>()
    };
    assert!(matches!(
        named_drops("leftOwner").as_slice(),
        [UnitDropPoint::BranchExit { branch: 1, .. }]
    ));
    assert!(matches!(
        named_drops("rightOwner").as_slice(),
        [UnitDropPoint::BranchExit { branch: 0, .. }]
    ));
    assert!(matches!(
        named_drops("borrowLeft").as_slice(),
        [UnitDropPoint::BranchExit { branch: 1, .. }]
    ));
    assert!(matches!(
        named_drops("borrowRight").as_slice(),
        [UnitDropPoint::BranchExit { branch: 0, .. }]
    ));
    assert!(matches!(
        named_drops("discardLeft").as_slice(),
        [UnitDropPoint::BranchExit { branch: 1, .. }]
    ));
    assert!(matches!(
        named_drops("discardRight").as_slice(),
        [UnitDropPoint::BranchExit { branch: 0, .. }]
    ));
    let borrowed_control = expression("if (flag) { borrowLeft } else { borrowRight }");
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| {
                fact.target() == UnitDropTarget::Temporary(borrowed_control)
                    && matches!(fact.point(), UnitDropPoint::CallReturn(_))
            })
            .count(),
        1,
        "Borrow receives one merged control temporary owner and drops it after the call"
    );
    let discarded_control = expression("if (flag) { discardLeft } else { discardRight }");
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| {
                fact.target() == UnitDropTarget::Temporary(discarded_control)
                    && fact.point() == UnitDropPoint::AfterExpression(discarded_control)
            })
            .count(),
        1,
        "a Read-context control owns and drops one merged result temporary"
    );
    ownership
        .clone()
        .validate()
        .expect("control result drop facts validate");
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
    .expect("reversed unit ownership product");
    assert_eq!(ownership.drops(), reversed.drops());
}

#[test]
fn consumed_control_result_updates_main_ownership_state() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun inspect(message: String): Unit {}\n\
         fun invalid(flag: Boolean): Unit {\n\
             val left = \"left\"\n\
             val right = \"right\"\n\
             val selected = if (flag) { left } else { right }\n\
             val reused = inspect(left)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "q/consumer.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0131"]);
    assert!(
        ownership.drops().is_empty(),
        "diagnostics prevent publishing a contradictory drop plan"
    );
}

#[test]
fn validated_unit_ownership_rejects_deferred_element_field_drop_plans() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         interface Finishable {\n\
             own fun finish(): Unit {}\n\
             own fun forward(): Unit { val result = finish() }\n\
         }\n\
         class Resource {}\n\
         class Holder(var payload: Resource)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.Holder\n\
         fun deferred(holders: List<Holder>): Unit {\n\
             val projected = holders[0].payload\n\
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

    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert_eq!(ownership.deferred().len(), 1);
    assert!(
        ownership.conditional_receiver_deliveries().is_empty(),
        "a deferred ownership boundary must not expose executable conditional deliveries"
    );
    assert_eq!(
        ownership.deferred()[0].reason(),
        OwnershipDeferredReason::IndexPlace
    );
    assert_eq!(
        sources
            .slice(
                consumer
                    .ast()
                    .expressions()
                    .get(ownership.deferred()[0].expression().expression())
                    .expect("deferred expression")
                    .span()
            )
            .expect("deferred source"),
        "holders[0].payload"
    );
    assert!(ownership.clone().validate().is_err());
}

#[test]
fn unit_non_null_assertion_consumption_and_source_restrictions() {
    // Source-qualified calls must enforce extraction independently of the result's Borrow mode.
    for (body, expected) in [
        (
            "fun test(own source: Resource?): Int { val first = read(source!!)\nreturn readNullable(source) }",
            vec!["L0131"],
        ),
        (
            "fun test(source: Resource?): Resource = source!!",
            vec!["L0133"],
        ),
        (
            "fun test(inout source: Resource?): Resource = source!!",
            vec!["L0133"],
        ),
        (
            "fun test(own source: Holder): Resource = source.item!!",
            vec!["L0132"],
        ),
        (
            "fun test(own source: Array<Resource?>): Resource = source[0]!!",
            vec!["L0136"],
        ),
        (
            "fun test(own source: Resource?): Int = use(source, source!!)",
            vec!["L0135"],
        ),
        (
            "fun test(source: Int?): Int { val first = source!!\nreturn first + source!! }",
            vec![],
        ),
    ] {
        let mut sources = SourceMap::new();
        let (provider_id, provider) = parsed(
            &mut sources,
            "provider.ko",
            "class Resource {}\nclass Holder(val item: Resource?) {}\nfun read(item: Resource): Int = 0\nfun readNullable(item: Resource?): Int = 0\nfun use(first: Resource?, second: Resource): Int = 0",
        );
        let (consumer_id, consumer) = parsed(&mut sources, "consumer.ko", body);
        let inputs = [
            SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
            SourceUnitInput::new("root", "consumer.ko", consumer_id, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert_eq!(diagnostic_codes(&ownership), expected, "{body}");
        assert_eq!(
            ownership.non_null_assertions().is_empty(),
            !expected.is_empty(),
            "{body}"
        );
        assert_eq!(ownership.validate().is_ok(), expected.is_empty(), "{body}");
    }
}

#[test]
fn unit_non_null_assertion_plans_bind_transfer_abort_and_result_drop() {
    use lang_frontend::{
        ownership_checking::NonNullAssertionTransferKind as Transfer,
        type_checking::AssertionFailureEffect,
    };
    let mut sources = SourceMap::new();
    let (provider_id, provider) = parsed(
        &mut sources,
        "assert-provider.ko",
        "class Resource {}\nfun create(): Resource? = Resource()\nfun read(item: Resource): Int = 0\nfun copied(source: Int?): Int = source!!",
    );
    let (consumer_id, consumer) = parsed(
        &mut sources,
        "assert-consumer.ko",
        "fun transferred(own source: Resource?): Resource = source!!\nfun temporary(): Int = read(create()!!)\nfun borrowedResult(own source: Resource?): Int = read(source!!)",
    );
    let inputs = [
        SourceUnitInput::new("root", "assert-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "assert-consumer.ko", consumer_id, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert_eq!(ownership.non_null_assertions().len(), 4);
    for plan in ownership.non_null_assertions() {
        let descriptor = plan.descriptor();
        assert_eq!(
            typed.types().non_null_assertion(descriptor.expression()),
            Some(*descriptor)
        );
        assert_eq!(
            ownership.non_null_assertion(descriptor.expression()),
            Some(plan)
        );
        assert_eq!(plan.null_effect(), AssertionFailureEffect::Abort);
        assert_eq!(
            plan.non_null_transfer(),
            if descriptor.copyability() == Copyability::Copyable {
                Transfer::Copy
            } else {
                Transfer::Consume
            }
        );
    }
    let consumer_unit = source_unit(&names, consumer_id);
    let temporary = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "create()!!"),
    );
    assert!(
        ownership
            .non_null_assertion(temporary)
            .unwrap()
            .source_place()
            .is_none()
    );
    let borrowed_drops = ownership
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), UnitDropTarget::Temporary(_)))
        .collect::<Vec<_>>();
    assert_eq!(borrowed_drops.len(), 2, "{:?}", ownership.drops());
    assert!(
        borrowed_drops
            .iter()
            .all(|fact| matches!(fact.point(), UnitDropPoint::CallReturn(_)))
    );
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| sources.slice(fact.value_origin()).unwrap() != "source")
    );
    let again = check_compilation_unit_ownership(
        &sources,
        &[inputs[1], inputs[0]],
        &names,
        &type_environment,
        &typed,
    )
    .unwrap();
    assert_eq!(ownership.non_null_assertions(), again.non_null_assertions());
    assert!(ownership.validate().is_ok());
}

#[test]
fn unit_non_null_assertion_plans_clear_after_later_assignment_failure() {
    // The RHS extraction must not survive a rejected immutable target in a validated product.
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-assignment.ko",
        "class Resource {}\nfun copied(source: Int?): Int = source!!\nfun invalid(own source: Resource?): Unit { val target = Resource()\n{ target = source!! } }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-assignment.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert_eq!(diagnostic_codes(&ownership), ["L0134"]);
    assert!(ownership.non_null_assertions().is_empty());
    assert!(ownership.validate().is_err());
}

#[test]
fn unit_non_null_assertion_plans_preserve_nested_and_assignment_transfers() {
    // A returned conditional owner and a replacement RHS each transfer exactly once.
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-control.ko",
        "class Resource {}\nfun selected(flag: Boolean, own left: Resource?, own right: Resource?): Resource = (if (flag) { left } else { right })!!\nfun replaced(own source: Resource?): Resource { var target = Resource()\n{ target = source!! }\nreturn target }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-control.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .unwrap();
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert_eq!(ownership.non_null_assertions().len(), 2);
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| !matches!(fact.target(), UnitDropTarget::Temporary(_)))
    );
    assert_eq!(
        ownership
            .drops()
            .iter()
            .filter(|fact| sources.slice(fact.value_origin()).unwrap() == "target")
            .count(),
        1
    );
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| sources.slice(fact.value_origin()).unwrap() != "source")
    );
    assert!(ownership.validate().is_ok());
}

#[test]
fn unit_non_null_assertion_loop_consumption_checks_only_reachable_backedges() {
    // A repeated extraction requires a restored owner; break and return cannot repeat it.
    for (body, expected) in [
        ("while (flag) { val item = source!! }", vec!["L0131"]),
        (
            "while (flag) { val item = source!!\ncontinue }",
            vec!["L0131"],
        ),
        ("loop { val item = source!! }", vec!["L0131"]),
        (
            "for (index in indices) { val item = source!! }",
            vec!["L0131"],
        ),
        ("while (check(source!!)) {}", vec!["L0131"]),
        ("while (flag) { val item = source!!\nbreak }", vec![]),
        ("loop { val item = source!!\nreturn 0 }", vec![]),
        (
            "while (flag) { val local: Resource? = Resource()\nval item = local!! }",
            vec![],
        ),
        (
            "var local = source\nwhile (flag) { val item = local!!\n{ local = Resource() } }",
            vec![],
        ),
        (
            "var local = source\nwhile (flag) { { local = Resource() }\nval item = local!! }",
            vec![],
        ),
        (
            "var local = source\nloop { { local = Resource() }\nif (flag) { break }\nval item = local!! }\nval result = local!!",
            vec![],
        ),
    ] {
        let mut sources = SourceMap::new();
        let (provider_id, provider) = parsed(
            &mut sources,
            "provider.ko",
            "class Resource {}\nfun check(item: Resource): Boolean = true",
        );
        let text = format!(
            "fun test(flag: Boolean, indices: Array<Int>, own source: Resource?): Int {{ {body}\nreturn 0 }}"
        );
        let (consumer_id, consumer) = parsed(&mut sources, "consumer.ko", &text);
        let inputs = [
            SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
            SourceUnitInput::new("root", "consumer.ko", consumer_id, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert_eq!(diagnostic_codes(&ownership), expected, "{body}");
        if !expected.is_empty() {
            assert_eq!(
                sources
                    .slice(ownership.diagnostics()[0].primary_span())
                    .unwrap(),
                "source"
            );
            assert!(ownership.non_null_assertions().is_empty());
        }
        assert_eq!(ownership.validate().is_ok(), expected.is_empty(), "{body}");
    }
}

#[test]
fn unit_pending_borrow_owner_survives_branches_and_nested_calls() {
    for operand in [
        "if (flag) { 1 } else { 2 }",
        "when (flag) { true -> 1; false -> 2 }",
        "if (flag) { read(first) } else { read(first) }",
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Resource {{}}\nfun read(item: Resource): Int = 1\nfun take(item: Resource, count: Int): Int = count\nfun inspect(own first: Resource, flag: Boolean): Int = take(first, {operand})"
        );
        let (source, file) = parsed(&mut sources, "pending.ko", &text);
        let inputs = [SourceUnitInput::new("root", "pending.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");
        assert!(
            ownership.diagnostics().is_empty(),
            "{operand}: {:?}",
            ownership.diagnostics()
        );
        let unit = source_unit(&names, source);
        let first = symbol_named(&ownership, &names, unit, "first");
        let call = UnitExpressionId::new(
            unit,
            expression_with_text(&sources, &file, &format!("take(first, {operand})")),
        );
        let drops = ownership
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(first))
            .map(|fact| fact.point())
            .collect::<Vec<_>>();
        assert_eq!(
            drops,
            [UnitDropPoint::CallReturn(call)],
            "the first argument remains borrowed until the outer call returns: {operand}"
        );
        ownership.validate().expect("valid ownership");
    }
}

#[test]
fn unit_pending_borrow_in_aborting_interpolation_has_no_normal_drop() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "pending-abort.ko",
        r#"
fun take(item: Rc<Int>, second: Int): Int = second
fun inspect(own first: Rc<Int>) {
    "${take(first, error("stop"))}"
}
"#,
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "pending-abort.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("ownership product terminates");
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    let first = symbol_named(&ownership, &names, source_unit(&names, source), "first");
    assert!(
        ownership
            .drops()
            .iter()
            .all(|fact| fact.target() != UnitDropTarget::Named(first)),
        "abort does not unwind the pending argument owner"
    );
    ownership.validate().expect("valid ownership");
}

#[test]
fn conditional_pending_receiver_drop_keeps_its_order_among_argument_and_local_owners() {
    for receiver in ["consume", "this.consume"] {
        let mut sources = SourceMap::new();
        let text = format!(
            "interface Relay {{ own fun consume(text: String, own flag: Boolean): Unit {{}}\nown fun relay(own flag: Boolean): Unit {{ val older = \"older\"\nval done = {receiver}(\"prefix\", if (flag) {{ val newer = \"newer\"\nif (flag) {{ return }} else {{ newer == \"newer\" }} }} else {{ true }})\nval used = println(older) }} }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &environment);
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        let point = UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            source_unit(&names, source),
            expression_with_text(&sources, &parsed, "return"),
        ));
        let fact = owned
            .ownership()
            .conditional_receiver_drops()
            .iter()
            .find(|fact| fact.point() == point)
            .unwrap();
        let origins = owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| fact.point() == point)
            .map(|fact| sources.slice(fact.value_origin()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(origins, ["newer", "\"prefix\"", "older"]);
        assert_eq!(
            fact.preceding_drops(),
            2,
            "new local and later operand precede pending receiver; older local follows it"
        );
    }
}

#[test]
fn function_value_callee_stays_owned_until_call_or_argument_exit() {
    for prefix in ["move ", ""] {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "main.ko",
            &format!(
                "fun entry(own flag: Boolean): Unit {{ val captured = \"captured\"\nval action: {prefix}(borrow String, own Boolean) -> Unit = {prefix}{{ text, accepted -> println(captured) }}\nval done = action(\"prefix\", if (flag) {{ return }} else {{ true }}) }}"
            ),
        );
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &environment);
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        let ownership = owned.ownership();
        let unit = source_unit(&names, source);
        let action = names.names().source_units()[unit.index()]
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "action")
            .unwrap()
            .id();
        let drops = ownership
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), UnitDropTarget::Named(symbol)
        if symbol.source_unit() == unit && symbol.symbol() == action)
            })
            .map(|fact| fact.point())
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            2,
            "callee must be cleaned on both return and successful call"
        );
        assert!(
            drops.contains(&UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &parsed, "return")
            )))
        );
        assert!(
            drops.contains(&UnitDropPoint::CallReturn(UnitExpressionId::new(
                unit,
                expression_with_text(
                    &sources,
                    &parsed,
                    "action(\"prefix\", if (flag) { return } else { true })"
                )
            )))
        );
        if prefix.is_empty() {
            let captured = names.names().source_units()[unit.index()]
                .resolution()
                .symbols()
                .iter()
                .find(|symbol| symbol.name() == "captured")
                .unwrap()
                .id();
            let points = ownership
                .drops()
                .iter()
                .filter(|fact| {
                    matches!(fact.target(), UnitDropTarget::Named(symbol)
            if symbol.source_unit() == unit && symbol.symbol() == captured)
                })
                .map(|fact| fact.point())
                .collect::<Vec<_>>();
            assert_eq!(points.len(), 2);
            assert!(
                points.iter().all(|point| matches!(
                    point,
                    UnitDropPoint::CallReturn(_) | UnitDropPoint::ControlTransfer(_)
                )),
                "shared source must survive every argument branch: {points:?}"
            );
        }
    }
}
