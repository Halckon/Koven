use super::*;

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
