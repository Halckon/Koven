//! SPEC-0067 callable 选择、实参映射与类型层面 place 分类测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{NameEnvironment, SymbolKind, resolve_names},
    parser::{Expression, ParsedFile, TypeRef},
    source::SourceMap,
    type_checking::{
        BuiltinType, CallableTarget, Capability, EnvironmentFunction, EnvironmentParameter,
        ExpressionCategory, ParameterMode, TypeEnvironment, TypeKind, check_types,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

const BUILTINS: [BuiltinType; 16] = [
    BuiltinType::Byte,
    BuiltinType::Short,
    BuiltinType::Int,
    BuiltinType::Long,
    BuiltinType::UByte,
    BuiltinType::UShort,
    BuiltinType::UInt,
    BuiltinType::ULong,
    BuiltinType::Float,
    BuiltinType::Double,
    BuiltinType::Boolean,
    BuiltinType::Char,
    BuiltinType::String,
    BuiltinType::Unit,
    BuiltinType::Nothing,
    BuiltinType::Any,
];

fn parsed(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("callable.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "callable type source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (sources, parsed)
}

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let declarations = BUILTINS.map(|builtin| {
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
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in declarations {
        types.bind_builtin(symbol, builtin).expect("binding");
    }
    for (symbol, capability) in capabilities {
        types
            .bind_capability(symbol, capability)
            .expect("capability binding");
    }
    (names, types)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn source_member_and_function_value_calls_record_stable_mappings() {
    let text = "fun combine(own first: Int, second: Int): Long = 1L\n\
                class Sample { fun convert(input: Int): Long = 1L }\n\
                fun use(sample: Sample, callback: (Int) -> Long): Long {\n\
                    val local = 1\n\
                    val combined = combine(local, second = borrow 2)\n\
                    val converted = sample.convert(3)\n\
                    return callback(4)\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(resolution.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.calls().len(), 3);
    assert!(matches!(
        typed.calls()[0].target(),
        CallableTarget::Source(_)
    ));
    assert_eq!(
        typed.calls()[0]
            .arguments()
            .iter()
            .map(|argument| argument.parameter_index())
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(typed.calls()[0].arguments()[0].mode(), ParameterMode::Value);
    assert_eq!(
        typed.calls()[0].arguments()[1].mode(),
        ParameterMode::Borrow
    );
    assert_eq!(
        typed.calls()[0].arguments()[0].category(),
        ExpressionCategory::Place
    );
    assert!(matches!(
        typed.calls()[1].target(),
        CallableTarget::Source(_)
    ));
    assert_eq!(typed.calls()[2].target(), CallableTarget::FunctionValue);
    assert!(typed.calls().iter().all(|call| matches!(
        typed.types().get(call.return_type()),
        Some(TypeKind::Builtin(BuiltinType::Long))
    )));
}

#[test]
fn parameter_modes_accept_only_the_phase2_contract_matrix() {
    let text = "fun read(input: Int): Unit {}\n\
                fun consume(own input: Int): Unit {}\n\
                fun mutate(inout input: Int): Unit {}\n\
                fun use(): Unit {\n\
                    var local = 1\n\
                    val automatic = read(local)\n\
                    val explicit = read(borrow 2)\n\
                    val consumed = consume(local)\n\
                    val changed = mutate(&local)\n\
                    val temporary = mutate(&3)\n\
                    val wrong = read(&local)\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(codes(typed.diagnostics()), ["L0122", "L0122"]);
    assert_eq!(typed.calls().len(), 4);
    assert_eq!(typed.calls()[2].arguments()[0].mode(), ParameterMode::Value);
    assert_eq!(
        typed.calls()[3].arguments()[0].category(),
        ExpressionCategory::Place
    );
    assert_eq!(typed.calls()[3].arguments()[0].mode(), ParameterMode::Inout);
}

#[test]
fn expected_lambdas_adopt_and_publish_all_parameter_modes() {
    let text = "fun inspect(borrow input: Int): Unit {}\n\
                fun mutate(inout input: Int): Unit {}\n\
                fun consume(own input: Int): Unit {}\n\
                fun applyBorrow(callback: (borrow Int) -> Unit): Unit {}\n\
                fun applyOwn(callback: (own Int) -> Unit): Unit {}\n\
                fun applyInout(callback: (inout Int) -> Unit): Unit {}\n\
                fun use(): Unit {\n\
                    val reader: (borrow Int) -> Unit = { item -> inspect(item) }\n\
                    val writer: (inout Int) -> Unit = { item -> mutate(&item) }\n\
                    val owner: (own Int) -> Unit = { item -> }\n\
                    val moved: move (borrow Int) -> Unit = move { item -> inspect(item) }\n\
                    val appliedBorrow = applyBorrow({ item -> })\n\
                    val appliedOwn = applyOwn({ item -> })\n\
                    val appliedInout = applyInout({ item -> })\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

    let named_modes = resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.kind() == SymbolKind::ValueParameter)
        .map(|symbol| (symbol.name(), typed.parameter_mode(symbol.id())))
        .collect::<Vec<_>>();
    assert_eq!(
        named_modes,
        [
            ("input", Some(ParameterMode::Borrow)),
            ("input", Some(ParameterMode::Inout)),
            ("input", Some(ParameterMode::Value)),
            ("callback", Some(ParameterMode::Borrow)),
            ("callback", Some(ParameterMode::Borrow)),
            ("callback", Some(ParameterMode::Borrow)),
        ]
    );

    let lambda_modes = resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
        .map(|symbol| typed.parameter_mode(symbol.id()))
        .collect::<Vec<_>>();
    assert_eq!(
        lambda_modes,
        [
            Some(ParameterMode::Borrow),
            Some(ParameterMode::Inout),
            Some(ParameterMode::Value),
            Some(ParameterMode::Borrow),
            Some(ParameterMode::Borrow),
            Some(ParameterMode::Value),
            Some(ParameterMode::Inout),
        ]
    );

    let lambda_type_modes = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::Lambda { .. }).then_some(id))
        .map(|id| {
            match typed
                .expression_type(id)
                .and_then(|ty| typed.types().get(ty))
            {
                Some(TypeKind::Function { parameters, .. }) => parameters[0].mode,
                other => panic!("expected typed lambda function, got {other:?}"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lambda_type_modes,
        [
            ParameterMode::Borrow,
            ParameterMode::Inout,
            ParameterMode::Value,
            ParameterMode::Borrow,
            ParameterMode::Borrow,
            ParameterMode::Value,
            ParameterMode::Inout,
        ]
    );
}

#[test]
fn implicit_and_explicit_borrow_function_types_share_one_identity() {
    let text = "val implicit: (Int) -> Unit = { input -> }\n\
                val explicit: (borrow Int) -> Unit = { input -> }\n\
                val owned: (own Int) -> Unit = { input -> }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

    let function_types = parsed
        .ast()
        .type_refs()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), TypeRef::Function { .. }))
        .map(|(id, _)| typed.type_ref_type(id).expect("function type fact"))
        .collect::<Vec<_>>();
    assert_eq!(function_types.len(), 3);
    assert_eq!(function_types[0], function_types[1]);
    assert_ne!(function_types[0], function_types[2]);
    assert!(matches!(
        typed.types().get(function_types[0]),
        Some(TypeKind::Function { parameters, .. })
            if parameters[0].mode == ParameterMode::Borrow
    ));
    assert!(matches!(
        typed.types().get(function_types[2]),
        Some(TypeKind::Function { parameters, .. })
            if parameters[0].mode == ParameterMode::Value
    ));
}

#[test]
fn structurally_invalid_expected_lambda_does_not_publish_a_mode() {
    let text = "fun inspect(borrow input: Int): Unit {}\n\
                val wrongMove: (borrow Int) -> Unit = move { item -> inspect(item) }\n\
                val wrongArity: (borrow Int) -> Unit = { -> }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(codes(typed.diagnostics()), ["L0084", "L0084"]);
    let invalid_lambda_parameter = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
        .expect("wrongMove lambda parameter");
    assert_eq!(typed.parameter_mode(invalid_lambda_parameter.id()), None);
}

#[test]
fn named_arity_type_and_target_failures_are_distinct() {
    let text = "fun take(first: Int, second: Int): Unit {}\n\
                fun use(callback: (Int) -> Unit): Unit {\n\
                    val local = 1\n\
                    val unknown = take(third = 1, second = 2)\n\
                    val ordered = take(first = 1, 2)\n\
                    val missing = take(1)\n\
                    val extra = take(1, 2, 3)\n\
                    val mismatch = take(true, 2)\n\
                    val namedFunctionValue = callback(input = 1)\n\
                    val nonCallable = local()\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(
        codes(typed.diagnostics()),
        [
            "L0120", "L0120", "L0121", "L0121", "L0084", "L0120", "L0119"
        ]
    );
}

#[test]
fn overloads_filter_by_type_then_report_no_match_or_ambiguity() {
    let text = "fun choose(input: Int): Int = 1\n\
                fun choose(input: Long): Long = 1L\n\
                fun nullable(input: Int?): Int = 1\n\
                fun nullable(input: Long?): Long = 1L\n\
                fun use(): Unit {\n\
                    val absent: Nothing? = null\n\
                    val noMatch = choose(true)\n\
                    val ambiguous = nullable(absent)\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(codes(typed.diagnostics()), ["L0123", "L0124"]);
}

#[test]
fn external_singleton_calls_preserve_predeclared_owned_and_borrow_contracts() {
    let (sources, parsed) = parsed(
        "val consumed: Long = consumeExternal(1)\n\
         val inspected: Long = inspectExternal(2)",
    );
    let (mut names, _) = environments();
    let consume = names
        .declare_function("consumeExternal")
        .expect("consume external");
    let inspect = names
        .declare_function("inspectExternal")
        .expect("inspect external");
    let mut types = TypeEnvironment::new(&names);
    for builtin in BUILTINS {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == builtin.name())
            .expect("builtin symbol")
            .id();
        types.bind_builtin(symbol, builtin).expect("binding");
    }
    for (symbol, mode) in [
        (consume, ParameterMode::Value),
        (inspect, ParameterMode::Borrow),
    ] {
        types
            .bind_function(
                symbol,
                EnvironmentFunction {
                    parameters: vec![EnvironmentParameter {
                        mode,
                        ty: lang_frontend::type_checking::EnvironmentType::Builtin(
                            BuiltinType::Int,
                        ),
                    }],
                    return_type: lang_frontend::type_checking::EnvironmentType::Builtin(
                        BuiltinType::Long,
                    ),
                    effects: Vec::new(),
                },
            )
            .expect("function binding");
    }
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty());
    assert!(matches!(
        typed.calls()[0].target(),
        CallableTarget::External(target) if target == consume
    ));
    assert_eq!(typed.calls()[0].arguments()[0].mode(), ParameterMode::Value);
    assert!(matches!(
        typed.calls()[1].target(),
        CallableTarget::External(target) if target == inspect
    ));
    assert_eq!(
        typed.calls()[1].arguments()[0].mode(),
        ParameterMode::Borrow
    );
}

#[test]
fn explicit_inferred_and_member_generic_calls_publish_stable_instances() {
    let text = "class Holder<A, B> {
                    fun <T> select(own payload: T, fallback: A): T = payload
                }
                fun <T> identity(own input: T): T = input
                fun <T> unwrap(input: Holder<T?, Long>): T? = null
                fun <T> apply(input: T, callback: (T) -> T): T = callback(input)
                fun <T> invoke(callback: (T) -> T): T
                fun use(holder: Holder<Int, Long>, nested: Holder<Int?, Long>, callback: (Int) -> Int): Unit {
                    val explicit = identity<Int>(1)
                    val inferred = identity(1)
                    val member = holder.select<String>(\"value\", 1)
                    val nestedValue = unwrap(nested)
                    val lambdaValue = apply(1, { item -> item })
                    val functionValue = invoke(callback)
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.calls().len(), 7);

    let identity_calls = &typed.calls()[1..=2];
    assert_eq!(identity_calls[0].instance(), identity_calls[1].instance());
    assert!(matches!(
        typed.types().get(identity_calls[0].return_type()),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        typed
            .types()
            .get(identity_calls[0].instance().type_arguments()[0]),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));

    let member = &typed.calls()[3];
    assert_eq!(member.instance().type_arguments().len(), 3);
    assert!(matches!(
        typed.types().get(member.instance().type_arguments()[0]),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        typed.types().get(member.instance().type_arguments()[1]),
        Some(TypeKind::Builtin(BuiltinType::Long))
    ));
    assert!(matches!(
        typed.types().get(member.instance().type_arguments()[2]),
        Some(TypeKind::Builtin(BuiltinType::String))
    ));
    assert_eq!(member.arguments()[0].parameter_type(), member.return_type());
    assert!(matches!(
        typed.types().get(typed.calls()[4].return_type()),
        Some(TypeKind::Nullable(inner))
            if matches!(typed.types().get(*inner), Some(TypeKind::Builtin(BuiltinType::Int)))
    ));
    assert!(matches!(
        typed.types().get(typed.calls()[5].return_type()),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        typed.types().get(typed.calls()[6].return_type()),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
}

#[test]
fn generic_inference_does_not_use_return_context_and_reports_conflicts() {
    let text = "fun <T> same(first: T, second: T): T = first
                fun <T> factory(): T
                fun <T> identity(input: T): T = input
                fun use(): Unit {
                    val conflict = same(1, 1L)
                    val missing: Int = factory()
                    val arity = identity<Int, Long>(1)
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(codes(typed.diagnostics()), ["L0140", "L0140", "L0091"]);
    assert_eq!(
        sources.slice(typed.diagnostics()[0].primary_span()),
        Ok("1L")
    );
    assert_eq!(
        sources.slice(typed.diagnostics()[1].primary_span()),
        Ok("factory")
    );
    assert_eq!(
        sources.slice(typed.diagnostics()[2].primary_span()),
        Ok("Long")
    );
}

#[test]
fn generic_bound_failures_filter_overloads_without_leaking_trial_diagnostics() {
    let text = "interface Marker {}
                class Good : Marker {}
                class Bad {}
                fun <T: Marker> interfaceBound(input: T): Unit {}
                fun <T: Copyable> copyBound(input: T): Unit {}
                fun <T: Transferable> transferBound(input: T): Unit {}
                fun <T: Copyable> choose(input: T): Int = 1
                fun <T: Transferable> choose(input: T?): Long = 1L
                fun use(good: Good, bad: Bad, callback: (Int) -> Int, text: String?, absent: Nothing?): Unit {
                    val goodInterface = interfaceBound(good)
                    val badInterface = interfaceBound(bad)
                    val copyable = copyBound(1)
                    val notCopyable = copyBound(bad)
                    val transferable = transferBound(\"ok\")
                    val notTransferable = transferBound(callback)
                    val selected = choose(text)
                    val noMatch = choose(callback)
                    val ambiguous = choose(absent)
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0093", "L0115", "L0141", "L0123", "L0124"]
    );
    assert_eq!(
        sources.slice(typed.diagnostics()[0].primary_span()),
        Ok("bad")
    );
    assert_eq!(
        sources.slice(typed.diagnostics()[1].primary_span()),
        Ok("bad")
    );
    assert_eq!(
        sources.slice(typed.diagnostics()[2].primary_span()),
        Ok("callback")
    );
    let selected = typed
        .calls()
        .iter()
        .find(|call| {
            matches!(
                typed.types().get(call.return_type()),
                Some(TypeKind::Builtin(BuiltinType::Long))
            ) && call.instance().type_arguments().len() == 1
        })
        .expect("the Transferable overload must survive candidate filtering");
    assert!(matches!(selected.target(), CallableTarget::Source(_)));
}

#[test]
fn generic_source_calls_instantiate_while_reference_and_safe_calls_remain_deferred() {
    let text = "fun <T> identity(own input: T): T = input\n\
                fun mono(input: Int): Int = input\n\
                class Sample { fun read(input: Int): Int = input }\n\
                val generic = identity<Int>(1)\n\
                val reference = ::mono\n\
                val referenced = reference(1)\n\
                val optional: Sample? = null\n\
                val safe = optional?.read(1)";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty());
    let calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::Call { .. }).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 3);
    assert!(matches!(
        typed
            .expression_type(calls[0])
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(typed.call(calls[0]).is_some());
    for call in &calls[1..] {
        assert!(matches!(
            typed
                .expression_type(*call)
                .and_then(|ty| typed.types().get(ty)),
            Some(TypeKind::Deferred(_))
        ));
        assert!(typed.call(*call).is_none());
    }
}
