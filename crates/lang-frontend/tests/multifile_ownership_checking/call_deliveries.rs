use super::*;

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
