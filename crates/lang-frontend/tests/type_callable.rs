//! SPEC-0067 callable 选择、实参映射与类型层面 place 分类测试。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameEnvironment, SymbolKind, resolve_names},
    parser::{Expression, ParsedFile, TypeRef},
    source::SourceMap,
    type_checking::{
        BuiltinType, Capability, DeferredReason, EnvironmentFunction, EnvironmentParameter,
        ExpressionCategory, ParameterMode, TypeEnvironment, TypeKind, UnitCallTarget,
        UnitExpressionId, UnitTypeKind, UnitTypeRefId, check_types, standard_environments,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

#[path = "type_callable/unit_entries.rs"]
mod unit_entries;
use unit_entries::checked_entries;

#[path = "type_callable/inout_storage.rs"]
mod inout_storage;

#[path = "type_callable/lambda_modes.rs"]
mod lambda_modes;

#[path = "type_callable/expected_parameter_modes.rs"]
mod expected_parameter_modes;

#[path = "type_callable/overload_lambda_unit.rs"]
mod overload_lambda_unit;

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
fn standard_error_has_a_stable_abort_identity_and_nothing_result() {
    let text = "fun choose(flag: Boolean): Int = if (flag) 1 else error(\"failed\")";
    let (sources, parsed_file) = parsed(text);
    let (names, types) = standard_environments();
    for typed in checked_entries(&sources, &parsed_file, &names, &types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let [call] = typed.calls() else {
            panic!("one standard error call");
        };
        assert!(matches!(call.target(), UnitCallTarget::External(_)));
        assert!(call.aborts());
        assert_eq!(call.arguments()[0].mode(), ParameterMode::Borrow);
        assert!(matches!(
            typed.types().get(call.return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
        ));
    }

    let shadowing = "fun error(message: String): Nothing = error(message)";
    let (sources, shadowed_file) = parsed(shadowing);
    for typed in checked_entries(&sources, &shadowed_file, &names, &types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let [call] = typed.calls() else {
            panic!("one source error call");
        };
        assert!(matches!(call.target(), UnitCallTarget::Declaration(_)));
        assert!(!call.aborts());
    }

    let invalid = "fun missing(): Unit { val item = error() }\n\
                   fun wrong(): Unit { val item = error(1) }";
    let (sources, invalid_file) = parsed(invalid);
    for typed in checked_entries(&sources, &invalid_file, &names, &types) {
        assert_eq!(codes(typed.diagnostics()), ["L0121", "L0084"]);
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    sources
                        .slice(diagnostic.primary_span())
                        .expect("diagnostic span")
                })
                .collect::<Vec<_>>(),
            ["error()", "1"]
        );
        assert!(typed.calls().is_empty());
    }
}

#[test]
fn standard_println_has_a_stable_borrow_effect_and_source_shadowing_has_none() {
    let (sources, parsed_file) = parsed("fun output(): Unit { println(\"hello\") }");
    let (names, types) = standard_environments();
    for typed in checked_entries(&sources, &parsed_file, &names, &types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.calls().len(), 1);
        let call = &typed.calls()[0];
        assert!(matches!(call.target(), UnitCallTarget::External(_)));
        assert!(call.prints_line());
        assert!(!call.aborts());
        assert_eq!(call.arguments()[0].mode(), ParameterMode::Borrow);
        assert!(matches!(
            typed.types().get(call.return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Unit))
        ));
    }

    let shadowing =
        "fun println(message: String): Unit {}\nfun output(): Unit { println(\"hello\") }";
    let (sources, parsed_file) = parsed(shadowing);
    for typed in checked_entries(&sources, &parsed_file, &names, &types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let call = typed.calls().last().expect("source println call");
        assert!(matches!(call.target(), UnitCallTarget::Declaration(_)));
        assert!(!call.prints_line());
    }
}

#[test]
fn source_member_and_function_value_calls_record_stable_mappings() {
    let text = "fun combine(own first: Int, second: Int): Long = 1L\n\
                class Sample { fun convert(input: Int): Long = 1L }\n\
                fun use(sample: Sample, callback: (Int) -> Long): Long {\n\
                    val local = 1\n\
                    val combined = combine(local, second = 2)\n\
                    val converted = sample.convert(3)\n\
                    return callback(4)\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    for typed in checked_entries(&sources, &parsed, &names, &types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.calls().len(), 3);
        assert!(matches!(
            typed.calls()[0].target(),
            UnitCallTarget::Declaration(_)
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
            UnitCallTarget::Symbol(_)
        ));
        assert_eq!(typed.calls()[2].target(), UnitCallTarget::FunctionValue);
        assert!(typed.calls().iter().all(|call| matches!(
            typed.types().get(call.return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Long))
        )));
    }
}

#[test]
fn parameter_modes_accept_only_the_phase2_contract_matrix() {
    let text = "fun read(input: Int): Unit {}\n\
                fun consume(own input: Int): Unit {}\n\
                fun mutate(inout input: Int): Unit {}\n\
                fun use(): Unit {\n\
                    var local = 1\n\
                    val automatic = read(local)\n\
                    val temporaryBorrow = read(2)\n\
                    val consumed = consume(local)\n\
                    val changed = mutate(&local)\n\
                    val temporary = mutate(&3)\n\
                    val wrong = read(&local)\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    for typed in checked_entries(&sources, &parsed, &names, &types) {
        assert_eq!(codes(typed.diagnostics()), ["L0122", "L0122"]);
        assert_eq!(typed.calls().len(), 4);
        assert_eq!(typed.calls()[2].arguments()[0].mode(), ParameterMode::Value);
        assert_eq!(
            typed.calls()[3].arguments()[0].category(),
            ExpressionCategory::Place
        );
        assert_eq!(typed.calls()[3].arguments()[0].mode(), ParameterMode::Inout);
    }
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
    expected_parameter_modes::check(&sources, &parsed, &names, &types);
}

#[test]
fn implicit_and_explicit_borrow_function_types_share_one_identity() {
    let text = concat!(
        "fun observe(): Unit {\n",
        "    val implicit: (Int) -> Unit = { input -> }\n",
        "    val explicit: (borrow Int) -> Unit = { input -> }\n",
        "    val owned: (own Int) -> Unit = { input -> }\n",
        "}",
    );
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let function_refs = parsed
        .ast()
        .type_refs()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), TypeRef::Function { .. }))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let [implicit, explicit, owned] = function_refs.as_slice() else {
        panic!("three explicit function TypeRefs");
    };
    for typed in checked_entries(&sources, &parsed, &names, &types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let source = typed.signatures().declarations()[0].symbol().source_unit();
        let [implicit, explicit, owned] = [*implicit, *explicit, *owned].map(|id| {
            typed
                .type_ref_type(UnitTypeRefId::new(source, id))
                .expect("function type fact")
        });
        assert_eq!(implicit, explicit);
        assert_ne!(implicit, owned);
        assert!(matches!(
            typed.types().get(implicit),
            Some(UnitTypeKind::Function { parameters, .. })
                if parameters[0].mode() == ParameterMode::Borrow
        ));
        assert!(matches!(
            typed.types().get(owned),
            Some(UnitTypeKind::Function { parameters, .. })
                if parameters[0].mode() == ParameterMode::Value
        ));
    }
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
    for typed in checked_entries(&sources, &parsed, &names, &types) {
        assert_eq!(
            codes(typed.diagnostics()),
            [
                "L0120", "L0120", "L0121", "L0121", "L0084", "L0120", "L0119"
            ]
        );
    }
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
    let callee_spans =
        [("choose(true)", "choose"), ("nullable(absent)", "nullable")].map(|(call, callee)| {
            let start = text.find(call).expect("original call");
            sources
                .span(parsed.source_id(), start, start + callee.len())
                .unwrap()
        });
    for unit in checked_entries(&sources, &parsed, &names, &types) {
        assert_eq!(codes(unit.diagnostics()), ["L0123", "L0124"]);
        assert!(unit.calls().is_empty());
        for (diagnostic, callee_span) in unit.diagnostics().iter().zip(callee_spans) {
            assert_eq!(diagnostic.primary_span(), callee_span);
        }
    }
}

#[test]
fn nullable_bottom_adapts_without_widening_other_nullable_types() {
    let text = "fun use(absent: Nothing?, integer: Int?): Unit {
                    val nullableInt: Int? = absent
                    val nullableLong: Long? = absent
                    val nonNull: Int = absent
                    val incompatible: Long? = integer
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    for unit in checked_entries(&sources, &parsed, &names, &types) {
        assert_eq!(codes(unit.diagnostics()), ["L0084", "L0084"]);
        assert_eq!(
            sources.slice(unit.diagnostics()[0].primary_span()),
            Ok("absent")
        );
        assert_eq!(
            sources.slice(unit.diagnostics()[1].primary_span()),
            Ok("integer")
        );
    }
}

#[test]
fn external_singleton_calls_preserve_predeclared_owned_and_borrow_contracts() {
    let (sources, parsed) = parsed(
        "fun use(): Unit {\n\
             val consumed: Long = consumeExternal(1)\n\
             val inspected: Long = inspectExternal(2)\n\
         }\n\
         fun unreachable(): Unit { stopExternal(\"message\") }",
    );
    let (mut names, _) = environments();
    let consume = names
        .declare_function("consumeExternal")
        .expect("consume external");
    let inspect = names
        .declare_function("inspectExternal")
        .expect("inspect external");
    let stop = names
        .declare_function("stopExternal")
        .expect("stop external");
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
    types
        .bind_function(
            stop,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: lang_frontend::type_checking::EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: lang_frontend::type_checking::EnvironmentType::Builtin(
                    BuiltinType::Nothing,
                ),
                effects: Vec::new(),
            },
        )
        .expect("effect-free Nothing function binding");
    for typed in checked_entries(&sources, &parsed, &names, &types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.calls().len(), 3);
        assert!(matches!(
            typed.calls()[0].target(),
            UnitCallTarget::External(target) if target == consume
        ));
        assert_eq!(typed.calls()[0].arguments()[0].mode(), ParameterMode::Value);
        assert!(matches!(
            typed.calls()[1].target(),
            UnitCallTarget::External(target) if target == inspect
        ));
        assert_eq!(
            typed.calls()[1].arguments()[0].mode(),
            ParameterMode::Borrow
        );
        let stop_call = &typed.calls()[2];
        assert!(matches!(
            stop_call.target(),
            UnitCallTarget::External(target) if target == stop
        ));
        assert_eq!(stop_call.arguments()[0].mode(), ParameterMode::Borrow);
        assert!(matches!(
            typed.types().get(stop_call.return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
        ));
        assert!(!stop_call.aborts());
    }
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
    for (names, unit) in unit_entries::checked_named_entries(&sources, &parsed, &names, &types) {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(unit.calls().len(), 7);
        let call = |text| {
            unit.calls()
                .iter()
                .find(|call| {
                    let expression = call.expression().expression();
                    let node = parsed.ast().expressions().get(expression).unwrap();
                    sources.slice(node.span()) == Ok(text)
                })
                .expect("call descriptor for original expression")
        };
        let int = UnitTypeKind::Builtin(BuiltinType::Int);
        let explicit = call("identity<Int>(1)");
        let inferred = call("identity(1)");
        assert_eq!(explicit.instance(), inferred.instance());
        assert_eq!(unit.types().get(explicit.return_type()), Some(&int));
        assert_eq!(unit.types().get(inferred.return_type()), Some(&int));
        let [argument] = explicit.instance().type_arguments() else {
            panic!("one identity type argument")
        };
        assert_eq!(unit.types().get(*argument), Some(&int));

        let member = call("holder.select<String>(\"value\", 1)");
        assert_eq!(
            member
                .instance()
                .type_arguments()
                .iter()
                .map(|&argument| unit.types().get(argument))
                .collect::<Vec<_>>(),
            [
                Some(&int),
                Some(&UnitTypeKind::Builtin(BuiltinType::Long)),
                Some(&UnitTypeKind::Builtin(BuiltinType::String)),
            ]
        );
        assert_eq!(member.arguments()[0].parameter_type(), member.return_type());
        assert_eq!(
            unit.types().get(member.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::String))
        );
        assert!(matches!(
            unit.types().get(call("unwrap(nested)").return_type()),
            Some(UnitTypeKind::Nullable(inner)) if unit.types().get(*inner) == Some(&int)
        ));
        for text in ["apply(1, { item -> item })", "invoke(callback)"] {
            assert_eq!(unit.types().get(call(text).return_type()), Some(&int));
        }
        let UnitCallTarget::Declaration(identity) = explicit.target() else {
            panic!("top-level identity declaration")
        };
        let UnitCallTarget::Symbol(select) = member.target() else {
            panic!("Holder.select member symbol")
        };
        let source = &names.source_units()[0];
        for (id, expected) in [
            (names.declaration_symbol(identity).unwrap(), "identity"),
            (select, "select"),
        ] {
            assert_eq!(id.source_unit(), source.source_unit());
            let symbol = source
                .resolution()
                .symbols()
                .iter()
                .find(|symbol| symbol.id() == id.symbol())
                .expect("selected original declaration symbol");
            assert_eq!(symbol.name(), expected);
        }
        let callback = call("callback(input)");
        assert_eq!(callback.target(), UnitCallTarget::FunctionValue);
        assert!(callback.instance().type_arguments().is_empty());
    }
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
    for typed in checked_entries(&sources, &parsed, &names, &types) {
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
    for unit in checked_entries(&sources, &parsed, &names, &types) {
        assert_eq!(
            codes(unit.diagnostics()),
            ["L0093", "L0115", "L0141", "L0123", "L0124"]
        );
        for (diagnostic, (call, argument)) in unit.diagnostics().iter().zip([
            ("interfaceBound(bad)", "bad"),
            ("copyBound(bad)", "bad"),
            ("transferBound(callback)", "callback"),
        ]) {
            let start = text.find(call).unwrap() + call.find('(').unwrap() + 1;
            let expected = sources
                .span(parsed.source_id(), start, start + argument.len())
                .unwrap();
            assert_eq!(diagnostic.primary_span(), expected);
        }
        let mut calls = unit
            .calls()
            .iter()
            .map(|call| {
                let node = parsed
                    .ast()
                    .expressions()
                    .get(call.expression().expression())
                    .unwrap();
                (
                    node.span().start(),
                    sources.slice(node.span()).unwrap(),
                    call,
                )
            })
            .collect::<Vec<_>>();
        calls.sort_by_key(|(start, _, _)| *start);
        assert_eq!(
            calls.iter().map(|(_, text, _)| *text).collect::<Vec<_>>(),
            [
                "interfaceBound(good)",
                "copyBound(1)",
                "transferBound(\"ok\")",
                "choose(text)"
            ]
        );
        let chosen = calls.last().unwrap().2;
        assert!(matches!(chosen.target(), UnitCallTarget::Declaration(_)));
        assert_eq!(chosen.instance().type_arguments().len(), 1);
        assert_eq!(
            unit.types().get(chosen.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Long))
        );
    }
}

#[test]
fn overload_lambda_trial_commits_only_the_unique_candidate_and_nested_call() {
    let text = "fun resolve(callback: (Int) -> Int): Int = 1
                fun resolve(callback: (String) -> String): String = \"text\"
                fun intResult(input: Int): Int = input
                fun use(): Unit {
                    val selected = resolve { intResult(it) }
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    overload_lambda_unit::unique_trial(&sources, &parsed, &names, &types);
}

#[test]
fn ambiguous_and_failed_overload_lambda_trials_leak_no_candidate_facts() {
    let text = "fun resolve(callback: (Int) -> Int): Int = 1
                fun resolve(callback: (String) -> String): String = \"text\"
                fun use(): Unit {
                    val ambiguous = resolve { it }
                    val noMatch = resolve { true }
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    overload_lambda_unit::failed_trials(&sources, &parsed, &names, &types);
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    let repeated = check_types(&sources, &parsed, &resolution, &types).expect("repeated types");
    assert_eq!(codes(typed.diagnostics()), ["L0124", "L0123"]);
    assert_eq!(typed.diagnostics(), repeated.diagnostics());
    assert_eq!(typed.calls(), repeated.calls());
    assert!(typed.calls().is_empty());
    for parameter in resolution
        .symbols()
        .iter()
        .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
    {
        assert_eq!(typed.parameter_mode(parameter.id()), None);
        // TypedFile seals missing symbol types as Error; a trial must not retain its candidate type.
        assert_eq!(
            typed
                .symbol_type(parameter.id())
                .and_then(|ty| typed.types().get(ty)),
            Some(&TypeKind::Error)
        );
    }
    for (id, node) in parsed.ast().expressions().iter().filter(|(_, node)| {
        matches!(
            node.payload(),
            Expression::Lambda { .. } | Expression::Call { .. }
        )
    }) {
        assert!(
            matches!(
                typed
                    .expression_type(id)
                    .and_then(|ty| typed.types().get(ty)),
                Some(TypeKind::Error)
            ),
            "trial fact leaked for {:?}",
            node.span()
        );
    }
}

#[test]
fn non_lambda_filter_to_one_candidate_preserves_direct_lambda_diagnostic() {
    let text = "fun choose(tag: Int, callback: (Int) -> Int): Int = 1
                fun choose(tag: String, callback: (String) -> String): String = \"text\"
                fun use(): Unit {
                    val invalid = choose(1, { item -> true })
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let callback = text.find("callback:").expect("chosen parameter name");
    let label_span = sources
        .span(parsed.source_id(), callback, callback + "callback".len())
        .unwrap();
    for unit in checked_entries(&sources, &parsed, &names, &types) {
        assert_eq!(codes(unit.diagnostics()), ["L0084"]);
        assert_eq!(
            sources.slice(unit.diagnostics()[0].primary_span()),
            Ok("true")
        );
        assert!(unit.calls().is_empty());
        assert!(unit.diagnostics()[0].details().iter().any(|detail| {
            matches!(detail, DiagnosticDetail::Label(label) if label.span() == label_span)
        }));
    }
}

#[test]
fn generic_overload_trials_preserve_named_multi_lambda_modes_and_mapping_order() {
    // The first trial types the first two lambdas with different modes, then
    // fails on the last lambda. Only the second trial's facts may survive.
    let text = "fun <T, U> dispatch(
                    first: (own T) -> T,
                    second: (borrow String) -> String,
                    third: (own U) -> U
                ): String = \"text\"
                fun <T, U> dispatch(
                    first: (borrow T) -> T,
                    second: (own Int) -> Int,
                    third: (inout U) -> U
                ): Long = 1L
                fun use(): Unit {
                    val selected = dispatch<String, Int>(
                        third = { changed -> changed + 1 },
                        first = { borrowed -> borrowed },
                        second = { owned -> owned + 1 }
                    )
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    overload_lambda_unit::generic_trial(&sources, &parsed, &names, &types);
}

#[test]
fn generic_source_calls_instantiate_while_reference_and_safe_calls_remain_deferred() {
    let text = "fun <T> identity(own input: T): T = input
                fun mono(input: Int): Int = input
                class Sample { fun read(input: Int): Int = input }
                fun use(): Unit {
                    val generic = identity<Int>(1)
                    val reference = ::mono
                    val referenced = reference(1)
                    val optional: Sample? = null
                    val safe = optional?.read(1)
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::Call { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 3);
    let call = |text| {
        calls
            .iter()
            .find(|(_, node)| sources.slice(node.span()) == Ok(text))
            .expect("original call expression")
            .0
    };
    for (resolved, unit) in unit_entries::checked_named_entries(&sources, &parsed, &names, &types) {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        let source = resolved.source_units()[0].source_unit();
        let generic = UnitExpressionId::new(source, call("identity<Int>(1)"));
        assert_eq!(
            unit.expression_type(generic)
                .and_then(|ty| unit.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(unit.calls().len(), 1);
        assert!(unit.call(generic).is_some());
        for (text, reason) in [
            ("reference(1)", DeferredReason::OverloadSelection),
            ("optional?.read(1)", DeferredReason::MemberAccess),
        ] {
            let id = UnitExpressionId::new(source, call(text));
            assert_eq!(
                unit.expression_type(id).and_then(|ty| unit.types().get(ty)),
                Some(&UnitTypeKind::Deferred(reason))
            );
            assert!(unit.call(id).is_none());
        }
    }
}

#[path = "type_callable/expected_move_literals.rs"]
mod expected_move_literals;
