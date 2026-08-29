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
        OwnershipCheckingError, RcOwnershipEffectKind, Transferability,
        UnitCallArgumentOwnershipKind, UnitClosureCaptureSource, UnitLoanTarget,
        UnitValueDeliveryKind, check_compilation_unit_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, Copyability, TypeEnvironment, UnitExpressionId, UnitTypeKind,
        ValidatedCompilationUnitTypes, check_compilation_unit_types, standard_environments,
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
             fun closure(): () -> Unit = {\n\
                 val receiver = this\n\
                 val captured = inspect(resource)\n\
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

    assert_eq!(ownership.diagnostics().len(), 1);
    assert_eq!(ownership.diagnostics()[0].code().to_string(), "L0131");
    assert!(ownership.captures().is_empty());
    assert_eq!(ownership.closures().len(), 1);
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.rc_effects().is_empty());
    assert!(ownership.construction_plans().is_empty());
}
