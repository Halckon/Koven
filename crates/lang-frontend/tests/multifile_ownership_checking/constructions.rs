use super::*;

#[test]
fn runtime_generator_size_borrow_precedes_cross_file_initializer_nested_inout() {
    for container in ["Array", "List"] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\nfun initializer(inout size: Int): (Int) -> Int {\n\
             size = 2\nreturn ({ index -> index })\n}",
        );
        let text = format!(
            "package q\nfun invalid(): Unit {{ var size = 1\n\
             val items = {container}<Int>(size, p.initializer(&size))\n}}"
        );
        let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", &text);
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("conflicting construction returns an ownership recovery product");
        assert_eq!(owned.diagnostics().len(), 1);
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .expect("cross-file conflicting operand span"),
            "&"
        );
    }
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
             val generated = List<Int>(number, initializer)\n\
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
        expression_with_text(&sources, &consumer, "List<Int>(number, initializer)"),
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
