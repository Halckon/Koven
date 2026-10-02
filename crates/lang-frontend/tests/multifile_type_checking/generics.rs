use super::*;

#[test]
fn generic_body_type_refs_publish_complete_cross_file_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Marker\n\
         class Good : Marker\n\
         value class Pair<T>(val first: T, val second: T)\n\
         value class NeedsMarker<T: Marker>(val item: T)\n\
         value class NeedsCopy<T: Copyable>(val item: T)\n\
         value class NeedsTransfer<T: Transferable>(val item: T)\n\
         enum class Maybe<T> { Some(item: T), None }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(pair: Pair<Int>, boxed: Box<Pair<Int>>, list: List<String>, marked: NeedsMarker<Good>, copied: NeedsCopy<Int>, transferred: NeedsTransfer<String>, maybe: Maybe<Int>): Unit {\n\
             val localPair: Pair<Int> = pair\n\
             val localBox: Box<Pair<Int>> = boxed\n\
             val localList: List<String> = list\n\
             val localMarked: NeedsMarker<Good> = marked\n\
             val localCopied: NeedsCopy<Int> = copied\n\
             val localTransferred: NeedsTransfer<String> = transferred\n\
             val isSome = maybe is Maybe.Some<Int>\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("generic body type refs are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed generic body type refs are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.type_ref_types(), reverse.type_ref_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    for text in [
        "Pair<Int>",
        "Box<Pair<Int>>",
        "List<String>",
        "NeedsMarker<Good>",
        "NeedsCopy<Int>",
        "NeedsTransfer<String>",
    ] {
        let refs = type_refs_with_text(&sources, &uses, text);
        assert!(
            refs.len() >= 2,
            "expected signature and local refs for {text}"
        );
        assert!(refs.iter().all(|id| {
            forward
                .type_ref_type(UnitTypeRefId::new(
                    source_unit(&forward_names, uses_source),
                    *id,
                ))
                .is_some()
        }));
    }
    let uses_unit = source_unit(&forward_names, uses_source);
    let maybe_refs = type_refs_with_text(&sources, &uses, "Maybe<Int>");
    let case_refs = type_refs_with_text(&sources, &uses, "Maybe.Some<Int>");
    assert_eq!(maybe_refs.len(), 1);
    assert_eq!(case_refs.len(), 1);
    let maybe = forward
        .type_ref_type(UnitTypeRefId::new(uses_unit, maybe_refs[0]))
        .expect("generic root type fact");
    assert!(matches!(
        forward
            .type_ref_type(UnitTypeRefId::new(uses_unit, case_refs[0]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root, .. }) if *root == maybe
    ));
}

#[test]
fn generic_source_calls_publish_explicit_inferred_bound_and_lambda_instances() {
    let mut sources = SourceMap::new();
    let (api_source, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\n\
         interface Marker\n\
         class Good : Marker\n\
         value class Pair<T>(val first: T, val second: T)\n\
         fun <T> identity(own input: T): T\n\
         fun <T> tagged(tag: Byte, own input: T): T\n\
         fun <A, B> second(own first: A, own second: B): B\n\
         fun <T> apply(own input: T, transform: (borrow T) -> T): T\n\
         fun <T> fromList(items: List<T>): T\n\
         fun <T> fromNullable(input: T?): T\n\
         fun <T> fromCallback(callback: (borrow T) -> T): T\n\
         fun <T> map(own input: T, callback: (borrow T) -> Int): Int\n\
         fun <T> map(own input: T, callback: (borrow T) -> String): String\n\
         fun route(input: Int): Int\n\
         fun <T> route(input: List<T>): T\n\
         fun <T: Marker> marked(own input: T): T",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(pair: Pair<Int>, good: Good, items: List<Int>, nullable: Int?, callback: (borrow Int) -> Int): Unit {\n\
             val explicit = identity<Int>(1)\n\
             val contextual = tagged<Int>(1, 2)\n\
             val ordered = second(1, \"ordered\")\n\
             val inferred = identity(2)\n\
             val nested = identity(pair)\n\
             val transformed = apply(3, { item -> item })\n\
             val listItem = fromList(items)\n\
             val nullableItem = fromNullable(nullable)\n\
             val callbackItem = fromCallback(callback)\n\
             val overloadLambda = map(4, { item -> item + 1 })\n\
             val mixedOverload = route(items)\n\
             val bounded = marked(good)\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/api.ko", api_source, &api),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("generic source calls are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed generic source calls are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let call = |text: &str| {
        let expression = expression_with_text(&sources, &uses, text);
        forward
            .call(UnitExpressionId::new(uses_unit, expression))
            .expect("generic call descriptor")
    };
    for text in [
        "identity<Int>(1)",
        "tagged<Int>(1, 2)",
        "identity(2)",
        "apply(3, { item -> item })",
        "fromList(items)",
        "fromNullable(nullable)",
        "fromCallback(callback)",
        "map(4, { item -> item + 1 })",
        "route(items)",
    ] {
        let descriptor = call(text);
        assert_eq!(descriptor.instance().type_arguments().len(), 1);
        assert_eq!(
            forward
                .types()
                .get(descriptor.instance().type_arguments()[0]),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(
            forward.types().get(descriptor.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
    let pair = call("identity(pair)");
    assert!(matches!(
        forward
            .types()
            .get(pair.instance().type_arguments()[0]),
        Some(UnitTypeKind::Nominal { declaration: owner, arguments })
            if *owner == declaration(&forward_names, "Pair") && arguments.len() == 1
    ));
    let marked = call("marked(good)");
    assert!(matches!(
        forward
            .types()
            .get(marked.instance().type_arguments()[0]),
        Some(UnitTypeKind::Nominal { declaration: owner, arguments })
            if *owner == declaration(&forward_names, "Good") && arguments.is_empty()
    ));
    let ordered = call("second(1, \"ordered\")");
    assert_eq!(ordered.instance().type_arguments().len(), 2);
    assert_eq!(
        forward.types().get(ordered.instance().type_arguments()[0]),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert_eq!(
        forward.types().get(ordered.instance().type_arguments()[1]),
        Some(&UnitTypeKind::Builtin(BuiltinType::String))
    );
    assert_eq!(
        forward.types().get(ordered.return_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::String))
    );
    let transform = call("apply(3, { item -> item })").arguments()[1];
    assert!(matches!(
        forward.types().get(transform.parameter_type()),
        Some(UnitTypeKind::Function { parameters, return_type, .. })
            if parameters.len() == 1
                && parameters[0].mode() == ParameterMode::Borrow
                && forward.types().get(parameters[0].ty())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && forward.types().get(*return_type)
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    ));
}

#[test]
fn invalid_generic_source_calls_keep_single_candidate_diagnostics_and_recovery() {
    let mut sources = SourceMap::new();
    let (api_source, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\n\
         interface Marker\n\
         class Resource\n\
         value class Pair<T>(val first: T, val second: T)\n\
         fun <T> identity(own input: T): T\n\
         fun <A, B> second(own first: A, own second: B): B\n\
         fun <T> choose(own first: T, own second: T): T\n\
         fun <T> tagged(flag: Boolean, own input: T): T\n\
         fun <T> make(): T\n\
         fun plain(own input: Int): Int\n\
         fun <T: Marker> marked(own input: T): T\n\
         fun <T: Copyable> copied(own input: T): T\n\
         fun <T: Transferable> sent(own input: T): T\n\
         fun <T> overloaded(input: List<T>): Int\n\
         fun <T> overloaded(input: Pair<T>): String\n\
         fun <T> pick(input: T): Int\n\
         fun <T> pick(input: List<T>): String\n\
         fun <T> fromFactory(factory: () -> T): T",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun bad(resource: Resource, shared: Rc<Int>, items: List<Int>): Unit {\n\
             val extra = identity<Int, String>(1)\n\
             val nongeneric = plain<Int>(1)\n\
             val missingType = second<Int>(1, \"x\")\n\
             val unresolved: Int = make()\n\
             val conflict = choose(1, \"x\")\n\
             val fixedMismatch = tagged(1, 2)\n\
             val interfaceBound = marked(resource)\n\
             val copyBound = copied(resource)\n\
             val transferBound = sent(shared)\n\
             val noOverload = overloaded(1)\n\
             val ambiguous = pick(items)\n\
             val lambdaOnly = fromFactory({ 1 })\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/api.ko", api_source, &api),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("generic call failures stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed generic call failures stay in the recovery product");

    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0091", "L0091", "L0091", "L0140", "L0140", "L0084", "L0093", "L0115", "L0141",
            "L0123", "L0124", "L0140"
        ]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("generic call diagnostic span"))
            .collect::<Vec<_>>(),
        [
            "String",
            "Int",
            "second",
            "make",
            "\"x\"",
            "1",
            "resource",
            "resource",
            "shared",
            "overloaded",
            "pick",
            "fromFactory"
        ]
    );
    let labels = |diagnostic: &Diagnostic| {
        diagnostic
            .details()
            .iter()
            .filter_map(|detail| match detail {
                DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&forward.body_diagnostics()[2]), ["B"]);
    assert_eq!(labels(&forward.body_diagnostics()[5]), ["flag: Boolean"]);
    for diagnostic in &forward.body_diagnostics()[6..9] {
        assert_eq!(labels(diagnostic), ["T"]);
    }
    assert!(forward.calls().is_empty());
    assert!(forward.validate().is_err());
}

#[test]
fn unbound_external_body_type_keeps_single_file_deferred_recovery() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\nfun use(): Unit { val local: Opaque = return }",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("fresh external type name");
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unbound external type remains a deferred recovery fact");
    let refs = type_refs_with_text(&sources, &file, "Opaque");

    assert_eq!(refs.len(), 1);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(matches!(
        typed
            .type_ref_type(UnitTypeRefId::new(source_unit(&names, source), refs[0]))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_generic_body_type_refs_keep_existing_codes_and_recovery() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Marker\n\
         class Resource\n\
         value class Pair<T>(val first: T, val second: T)\n\
         value class NeedsMarker<T: Marker>(val item: T)\n\
         value class NeedsCopy<T: Copyable>(val item: T)\n\
         value class NeedsTransfer<T: Transferable>(val item: T)\n\
         value class Loop(val next: Loop)\n\
         enum class Maybe<T> { Some(item: T), None }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun <T> invalid(): Unit {\n\
             val builtin: Int<String> = return\n\
             val nested: Pair<Int<String>> = return\n\
             val tooMany: Pair<Int, String> = return\n\
             val arity: Pair = return\n\
             val parameter: T<Int> = return\n\
             val capability: Copyable<Int> = return\n\
             val interfaceValue: Marker = return\n\
             val interfaceBound: NeedsMarker<Resource> = return\n\
             val interfaceAny: NeedsMarker<Any> = return\n\
             val copyBound: NeedsCopy<Resource> = return\n\
             val copyAny: NeedsCopy<Any> = return\n\
             val transferBound: NeedsTransfer<Rc<Int>> = return\n\
             val transferAny: NeedsTransfer<Any> = return\n\
             val badBox: Box<Resource> = return\n\
             val badList: List<Any> = return\n\
             val badInline: List<Loop> = return\n\
             val case: Maybe.Some = return\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("invalid generic body refs stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed invalid generic body refs stay in the recovery product");

    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0082", "L0082", "L0091", "L0091", "L0091", "L0082", "L0094", "L0093", "L0093",
            "L0115", "L0115", "L0141", "L0141", "L0117", "L0125", "L0125", "L0114"
        ]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        [
            "String",
            "String",
            "Pair",
            "Pair",
            "Int",
            "Int",
            "Marker",
            "Resource",
            "Any",
            "Resource",
            "Any",
            "Rc<Int>",
            "Any",
            "Resource",
            "Any",
            "Loop",
            "Maybe.Some"
        ]
    );
    let l0082_labels = forward
        .body_diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code().to_string() == "L0082")
        .flat_map(|diagnostic| diagnostic.details())
        .filter_map(|detail| match detail {
            DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(l0082_labels, ["Int", "Int", "Copyable"]);
    let nested = type_refs_with_text(&sources, &uses, "Pair<Int<String>>");
    assert_eq!(nested.len(), 1);
    assert!(matches!(
        forward
            .type_ref_type(UnitTypeRefId::new(
                source_unit(&forward_names, uses_source),
                nested[0],
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Error)
    ));
    let loop_declaration = declaration(&forward_names, "Loop");
    assert!(
        forward
            .signatures()
            .invalid_inline_nominals()
            .contains(&loop_declaration)
    );
    assert!(forward.validate().is_err());
}
