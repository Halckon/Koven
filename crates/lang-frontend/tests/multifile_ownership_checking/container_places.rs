use super::*;

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
