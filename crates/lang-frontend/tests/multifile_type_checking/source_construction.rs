use super::*;

#[test]
fn cross_file_source_constructions_publish_instantiated_value_mappings() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Marker<T> {}\n\
         value class Pair<T>(val first: T, val second: T)\n\
         enum class Maybe<T> { Some(item: T), None }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun marker(): Marker<Int> = Marker()\n\
         fun pair(): Pair<Int> = Pair(second = 2, first = 1)\n\
         fun some(): Maybe<Int> = Maybe.Some(3)\n\
         fun none(): Maybe<Int> = Maybe.None",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("source constructions type check internally");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed source constructions type check internally");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.diagnostics(), reverse.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.constructions(), reverse.constructions());
    assert!(typed.calls().is_empty());
    assert_eq!(typed.constructions().len(), 4);
    assert_eq!(
        typed.constructions()[0].target(),
        UnitConstructionTarget::Nominal(declaration(&names, "Marker"))
    );
    let pair = &typed.constructions()[1];
    assert_eq!(pair.instance().type_arguments().len(), 1);
    assert_eq!(
        pair.arguments()
            .iter()
            .map(|argument| (
                argument.parameter_name(),
                argument.evaluation_index(),
                argument.mode(),
            ))
            .collect::<Vec<_>>(),
        [
            ("first", 1, ParameterMode::Value),
            ("second", 0, ParameterMode::Value),
        ]
    );
    assert!(matches!(
        typed.constructions()[2].target(),
        UnitConstructionTarget::EnumCase(_)
    ));
    assert!(typed.constructions()[3].arguments().is_empty());
    for descriptor in typed.constructions() {
        assert_eq!(
            typed.expression_type(descriptor.expression()),
            Some(descriptor.result_type())
        );
        assert!(descriptor.arguments().iter().all(|argument| {
            argument
                .parameter_symbol()
                .expect("source construction parameter symbol")
                .source_unit()
                == source_unit(&names, models_source)
                && argument.argument().source_unit() == source_unit(&names, uses_source)
                && argument.category() == ExpressionCategory::Temporary
        }));
    }
}

#[test]
fn invalid_cross_file_source_constructions_keep_existing_diagnostics_and_no_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Service\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         class Resource {}\n\
         class C {}\n\
         class Pair(val first: Int, val second: Int)\n\
         class Mixed<T>(val inferred: T, val fixed: Int)\n\
         class Partial<A, B>(val a: A)\n\
         class Empty<T> {}\n\
         class NeedsCopy<T: Copyable> {}",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun invalidService(): Unit { Service() }\n\
         fun invalidEnum(): Unit { Maybe<Int>() }\n\
         fun named(): Unit { Pair(missing = 1, second = 2) }\n\
         fun arity(): Unit { Pair(1) }\n\
         fun mode(): Unit { Pair(&1, 2) }\n\
         fun typed(): Unit { Pair(true, 2) }\n\
         fun inferredThenTyped(): Unit { Mixed(1, true) }\n\
         fun badResult(): Int = C()\n\
         fun partial(): Unit { Partial(1) }\n\
         fun underconstrained(): Unit { Empty() }\n\
         fun bound(): Unit { NeedsCopy<Resource>() }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("construction errors stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0143", "L0143", "L0120", "L0121", "L0122", "L0084", "L0084", "L0084", "L0144",
            "L0144", "L0115"
        ]
    );
    let partial = typed
        .body_diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.code().to_string() == "L0144"
                && sources.slice(diagnostic.primary_span()) == Ok("Partial")
        })
        .expect("Partial inference diagnostic");
    assert_eq!(
        partial
            .details()
            .iter()
            .filter_map(|detail| match detail {
                DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .collect::<Vec<_>>(),
        ["B"]
    );
    assert!(typed.constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn deferred_explicit_constructor_type_arguments_publish_no_construction_fact() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         class Holder<T> {}\n\
         class Wrapper<T> {}\n\
         class Consumer<T>(val item: T)\n\
         class Broken(val item: Opaque)\n\
         fun explicit(): Unit { Holder<Opaque>() }\n\
         fun broken(): Unit { Broken(1) }\n\
         fun inferred(): Consumer<Wrapper<Opaque>> {\n\
             val wrapped: Wrapper<Opaque> = error(\"stop\")\n\
             return Consumer(wrapped)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("fresh unbound external type");
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unbound explicit type remains recoverable");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.constructions().is_empty());
}

#[test]
fn constructor_checks_operands_before_rejecting_its_result_type() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\nclass C(val item: Int)\nfun bad(): String = C(true)",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("operand mismatch remains a recoverable diagnostic");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).expect("span"),
            ))
            .collect::<Vec<_>>(),
        [("L0084".to_owned(), "true")]
    );
    assert!(typed.constructions().is_empty());
}

#[test]
fn invalid_nested_container_operand_stops_outer_construction_before_result_mismatch() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         class Holder(val item: List<Int>)\n\
         fun bad(): String = Holder(List(1))",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid nested container remains a recoverable diagnostic");
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).expect("span"),
            ))
            .collect::<Vec<_>>(),
        [("L0091".to_owned(), "List(1)")]
    );
    assert!(typed.container_constructions().is_empty());
    assert!(typed.constructions().is_empty());
}

#[test]
fn rejected_constructor_inference_still_checks_every_container_operand() {
    for (text, expected_codes) in [
        (
            "package p\n\
         class Resource {}\n\
         value class Point(val x: Int)\n\
         class NeedsCopy<T: Copyable>(val item: Int)\n\
         fun bad(): Unit { NeedsCopy<Resource>(List(1)) }",
            vec!["L0115".to_owned(), "L0091".to_owned()],
        ),
        (
            "package p\n\
         value class Point(val x: Int)\n\
         class Holder<T>(val action: () -> T)\n\
         fun bad(): Unit { Holder({ List(1) }) }",
            vec!["L0144".to_owned(), "L0091".to_owned()],
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "uses.ko", text);
        let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("rejected construction still checks invalid container operands");
        assert_eq!(
            typed
                .body_diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes
        );
        assert!(typed.container_constructions().is_empty());
        assert!(typed.constructions().is_empty());
    }
}

#[test]
fn cross_file_constructor_overload_trials_are_transactional_and_ignore_candidate_expected() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Marker<T> {}\n\
         fun choose(action: () -> Marker<Int>): Int = 1\n\
         fun choose(action: () -> Marker<Long>): Long = 1L",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun selected(): Int = choose({ Marker<Int>() })\n\
         fun rejected(): Unit { val result = choose({ Marker() }) }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("overload trials remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0123"]
    );
    assert_eq!(typed.constructions().len(), 1);
    assert_eq!(
        typed.constructions()[0].instance().type_arguments().len(),
        1
    );
}
