//! SPEC-0067 callable 选择、实参映射与类型层面 place 分类测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{NameEnvironment, resolve_names},
    parser::{Expression, ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        BuiltinType, CallableTarget, EnvironmentFunction, EnvironmentParameter, ExpressionCategory,
        ParameterMode, TypeEnvironment, TypeKind, check_types,
    },
};

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
    let lexed = lex(&sources, source).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
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
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in declarations {
        types.bind_builtin(symbol, builtin).expect("binding");
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
    let text = "fun combine(first: Int, borrow second: Int): Long = 1L\n\
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
    let text = "fun read(borrow input: Int): Unit {}\n\
                fun mutate(inout input: Int): Unit {}\n\
                fun use(): Unit {\n\
                    var local = 1\n\
                    val automatic = read(local)\n\
                    val explicit = read(borrow 2)\n\
                    val changed = mutate(&local)\n\
                    val temporary = mutate(&3)\n\
                    val wrong = read(&local)\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(codes(typed.diagnostics()), ["L0122", "L0122"]);
    assert_eq!(typed.calls().len(), 3);
    assert_eq!(
        typed.calls()[2].arguments()[0].category(),
        ExpressionCategory::Place
    );
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
fn external_singleton_call_uses_the_same_positional_contract() {
    let (sources, parsed) = parsed("val result: Long = external(1)");
    let (mut names, _) = environments();
    let external = names.declare_function("external").expect("external");
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
    types
        .bind_function(
            external,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Value,
                    ty: lang_frontend::type_checking::EnvironmentType::Builtin(BuiltinType::Int),
                }],
                return_type: lang_frontend::type_checking::EnvironmentType::Builtin(
                    BuiltinType::Long,
                ),
            },
        )
        .expect("function binding");
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty());
    assert!(matches!(
        typed.calls()[0].target(),
        CallableTarget::External(target) if target == external
    ));
}

#[test]
fn generic_reference_and_safe_calls_remain_explicitly_deferred() {
    let text = "fun <T> identity(input: T): T = input\n\
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
    for call in calls {
        assert!(matches!(
            typed
                .expression_type(call)
                .and_then(|ty| typed.types().get(ty)),
            Some(TypeKind::Deferred(_))
        ));
        assert!(typed.call(call).is_none());
    }
}
