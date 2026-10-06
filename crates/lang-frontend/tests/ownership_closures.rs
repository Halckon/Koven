//! SPEC-0032 closure capture identity and Transferability fact tests.

use std::{fs, path::Path};

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameEnvironment, NameResolution, SymbolId, resolve_names},
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, DropPoint, DropTarget,
        LoanKind, LoanTarget, OwnershipCheckedFile, Transferability, check_ownership,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        BuiltinType, Capability, EnvironmentFunction, EnvironmentFunctionEffect,
        EnvironmentParameter, EnvironmentType, IntrinsicTypeConstructor, ParameterMode,
        TypeEnvironment, TypedFile, check_types,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

#[path = "ownership_closures/callable_provenance.rs"]
mod callable_provenance;

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

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BUILTINS.map(|builtin| {
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
    let containers = [
        (
            names.declare_type("Box").expect("Box"),
            IntrinsicTypeConstructor::Box,
        ),
        (
            names.declare_type("Rc").expect("Rc"),
            IntrinsicTypeConstructor::Rc,
        ),
        (
            names.declare_type("Array").expect("Array"),
            IntrinsicTypeConstructor::Array,
        ),
        (
            names.declare_type("List").expect("List"),
            IntrinsicTypeConstructor::List,
        ),
        (
            names.declare_type("MutableList").expect("MutableList"),
            IntrinsicTypeConstructor::MutableList,
        ),
    ];
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, capability) in capabilities {
        types
            .bind_capability(symbol, capability)
            .expect("capability binding");
    }
    for (symbol, constructor) in containers {
        types
            .bind_intrinsic(symbol, constructor)
            .expect("intrinsic binding");
    }
    (names, types)
}

fn cross_thread_environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BUILTINS.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        )
    });
    let dispatch = names.declare_function("dispatch").expect("dispatch");
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    types
        .bind_function(
            dispatch,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Value,
                    ty: EnvironmentType::Function {
                        move_only: true,
                        parameters: Vec::new(),
                        return_type: Box::new(EnvironmentType::Builtin(BuiltinType::Unit)),
                    },
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::CrossThreadTransfer { parameter: 0 }],
            },
        )
        .expect("dispatch binding");
    (names, types)
}

fn checked(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    NameResolution,
    TypedFile,
    OwnershipCheckedFile,
) {
    let result = analyzed(text);
    assert!(
        result.4.diagnostics().is_empty(),
        "{:?}",
        result.4.diagnostics()
    );
    result
}

fn analyzed(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    NameResolution,
    TypedFile,
    OwnershipCheckedFile,
) {
    analyzed_with(text, environments())
}

fn analyzed_with(
    text: &str,
    (environment, types): (NameEnvironment, TypeEnvironment),
) -> (
    SourceMap,
    ParsedFile,
    NameResolution,
    TypedFile,
    OwnershipCheckedFile,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("closures.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "closure source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    (sources, parsed, names, typed, owned)
}

fn codes(owned: &OwnershipCheckedFile) -> Vec<String> {
    owned
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn symbol(names: &NameResolution, name: &str) -> SymbolId {
    names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .unwrap_or_else(|| panic!("missing symbol {name}"))
        .id()
}

fn lambdas(parsed: &ParsedFile) -> Vec<ExpressionId> {
    let mut lambdas = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some((node.span().start(), id))
        })
        .collect::<Vec<_>>();
    lambdas.sort_by_key(|(start, _)| *start);
    lambdas.into_iter().map(|(_, id)| id).collect()
}

fn ability(
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    name: &str,
) -> Transferability {
    let ty = typed
        .symbol_type(symbol(names, name))
        .unwrap_or_else(|| panic!("missing type for {name}"));
    owned
        .transferability(ty)
        .unwrap_or_else(|| panic!("missing transferability for {name}"))
}

#[test]
fn default_and_move_lambdas_publish_borrow_copy_and_move_facts() {
    let text = "class Resource {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun captures(own resource: Resource, number: Int): Unit {\n\
                    val shared: () -> Unit = {\n\
                        val first = number\n\
                        val second = inspect(resource)\n\
                        val again = number\n\
                    }\n\
                    val owned: move () -> Unit = move {\n\
                        val first = number\n\
                        val second = resource\n\
                    }\n\
                    val empty: () -> Unit = {}\n\
                }";
    let (_, parsed, names, _, owned) = checked(text);
    let lambdas = lambdas(&parsed);
    assert_eq!(lambdas.len(), 3);

    let resource = symbol(&names, "resource");
    let number = symbol(&names, "number");
    let shared = owned.captures_of(lambdas[0]).copied().collect::<Vec<_>>();
    assert_eq!(shared.len(), 2);
    assert_eq!(
        shared
            .iter()
            .map(|capture| capture.source())
            .collect::<Vec<_>>(),
        [
            ClosureCaptureSource::Symbol(number),
            ClosureCaptureSource::Symbol(resource)
        ]
    );
    assert!(
        shared
            .iter()
            .all(|capture| capture.mode() == ClosureCaptureMode::Shared
                && capture.effect() == ClosureCaptureEffect::Borrow)
    );

    let moved = owned.captures_of(lambdas[1]).copied().collect::<Vec<_>>();
    assert_eq!(moved.len(), 2);
    assert_eq!(moved[0].source(), ClosureCaptureSource::Symbol(number));
    assert_eq!(moved[0].effect(), ClosureCaptureEffect::Copy);
    assert_eq!(moved[1].source(), ClosureCaptureSource::Symbol(resource));
    assert_eq!(moved[1].effect(), ClosureCaptureEffect::Move);
    assert!(
        moved
            .iter()
            .all(|capture| capture.mode() == ClosureCaptureMode::Owned)
    );

    assert_eq!(
        owned.closure(lambdas[0]).expect("shared").transferability(),
        Transferability::NotTransferable
    );
    assert_eq!(
        owned.closure(lambdas[1]).expect("move").transferability(),
        Transferability::Transferable
    );
    assert_eq!(
        owned.closure(lambdas[2]).expect("empty").transferability(),
        Transferability::Transferable
    );
    assert!(owned.drops().iter().any(|drop| {
        matches!(drop.target(), DropTarget::Captured { closure, source: ClosureCaptureSource::Symbol(symbol), .. } if closure == lambdas[1] && symbol == resource)
    }));
    assert!(
        !owned
            .drops()
            .iter()
            .any(|drop| drop.target() == DropTarget::Named(resource))
    );
}

#[test]
fn implicit_it_is_a_value_parameter_and_never_a_capture() {
    let text = "class Resource {}\n\
                fun useResource(own item: Resource): Unit {}\n\
                fun apply(callback: (own Resource) -> Unit): Unit {}\n\
                fun use(): Unit {\n\
                    apply {\n\
                        val first = useResource(it)\n\
                        val second = useResource(it)\n\
                    }\n\
                }";
    let (_, parsed, names, typed, owned) = analyzed(text);
    let lambda = lambdas(&parsed)[0];
    let implicit = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "it")
        .expect("implicit it symbol");

    assert_eq!(
        typed.parameter_mode(implicit.id()),
        Some(ParameterMode::Value)
    );
    assert_eq!(owned.captures_of(lambda).count(), 0);
    assert_eq!(codes(&owned), ["L0131"]);
}

#[test]
fn implicit_borrow_and_inout_match_explicit_parameter_ownership() {
    let text = "class Resource {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun use(): Unit {\n\
                    val implicitBorrow: (borrow Resource) -> Unit = { val read = inspect(it) }\n\
                    val explicitBorrow: (borrow Resource) -> Unit = { item -> val read = inspect(item) }\n\
                    val implicitInout: (inout Int) -> Unit = { it = 2 }\n\
                    val explicitInout: (inout Int) -> Unit = { item -> item = 2 }\n\
                }";
    let (_, parsed, names, typed, owned) = analyzed(text);

    assert!(owned.diagnostics().is_empty());
    assert!(
        lambdas(&parsed)
            .into_iter()
            .all(|lambda| owned.captures_of(lambda).count() == 0)
    );
    assert_eq!(
        names
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind()
                == lang_frontend::name_resolution::SymbolKind::LambdaParameter)
            .map(|symbol| typed.parameter_mode(symbol.id()))
            .collect::<Vec<_>>(),
        [
            Some(ParameterMode::Borrow),
            Some(ParameterMode::Borrow),
            Some(ParameterMode::Inout),
            Some(ParameterMode::Inout),
        ]
    );
}

#[test]
fn nested_capture_uses_symbol_identity_and_shadowing_does_not_capture() {
    let text = "fun nested(number: Int): Unit {\n\
                    val outer: () -> Unit = {\n\
                        val local = 1\n\
                        val inner: () -> Unit = { val sum = number + local }\n\
                    }\n\
                    val shadow: (Int) -> Unit = { number -> val used = number }\n\
                }";
    let (_, parsed, names, _, owned) = checked(text);
    let lambdas = lambdas(&parsed);
    assert_eq!(lambdas.len(), 3);
    let number = symbol(&names, "number");
    let local = symbol(&names, "local");

    assert_eq!(
        owned
            .captures_of(lambdas[0])
            .map(|capture| capture.source())
            .collect::<Vec<_>>(),
        [ClosureCaptureSource::Symbol(number)]
    );
    assert_eq!(
        owned
            .captures_of(lambdas[1])
            .map(|capture| capture.source())
            .collect::<Vec<_>>(),
        [
            ClosureCaptureSource::Symbol(number),
            ClosureCaptureSource::Symbol(local)
        ]
    );
    assert_eq!(owned.captures_of(lambdas[2]).count(), 0);
}

#[test]
fn unqualified_field_reference_normalizes_to_this_capture() {
    let text = "class Counter(val count: Int) {\n\
                    fun reader() { val closure: () -> Unit = { val result = count } }\n\
                }";
    let (_, parsed, _, _, owned) = checked(text);
    let lambda = lambdas(&parsed)[0];
    let captures = owned.captures_of(lambda).copied().collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].source(), ClosureCaptureSource::This);
    assert_eq!(captures[0].mode(), ClosureCaptureMode::Shared);
    let escaping =
        "class Counter(val count: Int) { fun reader(): () -> Unit = { val result = count } }";
    assert_eq!(codes(&analyzed(escaping).4), ["L0137"]);
}

#[test]
fn shared_this_capture_loans_receiver_fields_until_last_closure_use() {
    let text = "fun run(action: () -> Unit): Unit {}\n\
                class Counter(var count: Int) {\n\
                    fun conflict(): Unit {\n\
                        val closure: () -> Unit = { val result = count }\n\
                        count = 2\n\
                        val invoked = run(closure)\n\
                    }\n\
                }";
    assert_eq!(codes(&analyzed(text).4), ["L0135"]);
}

#[test]
fn transferability_is_structural_and_independent_from_copyability() {
    let text = "value class Point(val x: Int)\n\
                class Packet(val point: Point)\n\
                class CallbackBox(val callback: () -> Unit)\n\
                enum class Choice { Value(packet: Packet), Empty }\n\
                fun <T : Transferable, U> abilities(\n\
                    packet: Packet, callback: CallbackBox, text: String, maybe: Packet?,\n\
                    packets: List<Packet>, callbacks: List<CallbackBox>, shared: Rc<Packet>, action: () -> Unit,\n\
                    choice: Choice, proven: T, unknown: U, top: Any\n\
                ): Unit {}";
    let (_, _, names, typed, owned) = checked(text);
    for name in ["packet", "text", "maybe", "packets", "choice", "proven"] {
        assert_eq!(
            ability(&names, &typed, &owned, name),
            Transferability::Transferable,
            "{name}"
        );
    }
    for name in [
        "callback",
        "callbacks",
        "shared",
        "action",
        "unknown",
        "top",
    ] {
        assert_eq!(
            ability(&names, &typed, &owned, name),
            Transferability::NotTransferable,
            "{name}"
        );
    }
    assert_eq!(
        ability(&names, &typed, &owned, "packet"),
        Transferability::Transferable,
        "ordinary classes can be transferable while remaining MoveOnly"
    );
}

#[test]
fn move_capture_consumes_owner_and_rejects_borrowed_or_this_sources() {
    let moved = "class Resource {}\n\
                 fun inspect(item: Resource): Unit {}\n\
                 fun consume(own item: Resource): Unit {\n\
                     val closure: move () -> Unit = move { val read = inspect(item) }\n\
                     val after = inspect(item)\n\
                 }";
    assert_eq!(codes(&analyzed(moved).4), ["L0131"]);

    let borrowed = "class Resource {}\n\
                    fun inspect(item: Resource): Unit {}\n\
                    fun invalid(item: Resource): Unit {\n\
                        val closure: move () -> Unit = move { val read = inspect(item) }\n\
                    }";
    let borrowed = analyzed(borrowed).4;
    assert_eq!(codes(&borrowed), ["L0138"]);
    assert!(borrowed.captures().is_empty());

    let receiver = "class Counter(val count: Int) {\n\
                        fun invalid(): move () -> Unit = move { val read = count }\n\
                    }";
    assert_eq!(codes(&analyzed(receiver).4), ["L0138"]);
}

#[test]
fn shared_capture_cannot_supply_an_exclusive_inout_receiver() {
    let (_, _, _, _, owned) = analyzed(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 } }\nfun run(): Unit { val cell = Cell(0)\nval f: () -> Unit = { cell.set() } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");

    let (_, _, _, _, owned) = analyzed(
        "fun mutate(inout n: Int): Unit { n = 1 }\nfun run(): Unit { var n = 0\nval f: () -> Unit = { mutate(&n) } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");

    let (_, _, _, _, owned) = analyzed(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 } }\nfun run(): Unit { val cell = Cell(0)\nval f: move () -> Unit = move { cell.set() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn implicit_member_call_captures_this_instead_of_creating_a_capture_free_closure() {
    let (_, _, _, _, owned) = checked(
        "class Counter(var n: Int) { borrow fun read(): Int = n\nborrow fun keep(): Unit { val f: () -> Unit = { val x = read() } } }",
    );
    assert!(owned.captures().iter().any(|capture| {
        capture.source() == ClosureCaptureSource::This
            && capture.mode() == ClosureCaptureMode::Shared
    }));

    let (_, _, _, _, owned) = analyzed(
        "class Counter(var n: Int) { borrow fun read(): Int = n\nborrow fun escape(): () -> Unit = { val x = read() } }",
    );
    assert_eq!(codes(&owned), ["L0137"]);

    let (_, _, _, _, owned) = analyzed(
        "class Counter(var n: Int) { borrow fun read(): Int = n\nborrow fun bad(): Unit { val f: move () -> Unit = move { val x = read() } } }",
    );
    assert_eq!(codes(&owned), ["L0138"]);

    let (_, _, _, _, owned) = analyzed(
        "class Resource { own fun consume(): Unit {}\nown fun bad(): Unit { val f: () -> Unit = { consume() } } }",
    );
    assert_eq!(codes(&owned), ["L0133"]);
}

#[test]
fn captured_names_cannot_be_moved_from_shared_or_assigned() {
    let moved = "class Resource {}\n\
                 fun take(own item: Resource): Unit {}\n\
                 fun invalid(own item: Resource): Unit {\n\
                     val closure: () -> Unit = { val moved = take(item) }\n\
                 }";
    assert_eq!(codes(&analyzed(moved).4), ["L0133"]);

    let assigned = "fun invalid(): Unit {\n\
                        var number = 1\n\
                        val closure: () -> Unit = { number = 2 }\n\
                    }";
    assert_eq!(codes(&analyzed(assigned).4), ["L0135"]);
}

#[test]
fn shared_capture_loan_ends_after_the_closure_last_use() {
    let passing = "class Resource {}\n\
                   fun inspect(item: Resource): Unit {}\n\
                   fun run(action: () -> Unit): Unit {}\n\
                   fun take(own item: Resource): Unit {}\n\
                   fun released(own item: Resource): Unit {\n\
                       val closure: () -> Unit = { val read = inspect(item) }\n\
                       val invoked = run(closure)\n\
                       val moved = take(item)\n\
                   }";
    checked(passing);

    let failing = "class Resource {}\n\
                   fun inspect(item: Resource): Unit {}\n\
                   fun run(action: () -> Unit): Unit {}\n\
                   fun take(own item: Resource): Unit {}\n\
                   fun conflict(own item: Resource): Unit {\n\
                       val closure: () -> Unit = { val read = inspect(item) }\n\
                       val moved = take(item)\n\
                       val invoked = run(closure)\n\
                   }";
    assert_eq!(codes(&analyzed(failing).4), ["L0135"]);

    let dropping = "class Resource {}\n\
                    fun inspect(item: Resource): Unit {}\n\
                    fun run(action: () -> Unit): Unit {}\n\
                    fun releaseOnly(own item: Resource): Unit {\n\
                        val closure: () -> Unit = { val read = inspect(item) }\n\
                        val invoked = run(closure)\n\
                    }";
    let (_, _, names, _, owned) = checked(dropping);
    let item = names
        .symbols()
        .iter()
        .rfind(|candidate| candidate.name() == "item")
        .expect("releaseOnly item")
        .id();
    let closure = symbol(&names, "closure");
    let call_return_drops = owned
        .drops()
        .iter()
        .filter_map(|drop| match drop.point() {
            DropPoint::CallReturn(_) => Some(drop.target()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        call_return_drops,
        [DropTarget::Named(closure), DropTarget::Named(item)]
    );
}

#[test]
fn borrowed_closure_may_stay_local_but_cannot_escape() {
    let text = "class Resource {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun deliver(own callback: () -> Unit): Unit {}\n\
                fun returnIt(own item: Resource): () -> Unit {\n\
                    val closure: () -> Unit = { val read = inspect(item) }\n\
                    return closure\n\
                }\n\
                fun passIt(own item: Resource): Unit {\n\
                    val closure: () -> Unit = { val read = inspect(item) }\n\
                    val sent = deliver(closure)\n\
                }";
    assert_eq!(codes(&analyzed(text).4), ["L0137", "L0137"]);
}

#[test]
fn only_compiler_bound_cross_thread_effect_requires_transferability() {
    let external = "class Local(val callback: () -> Unit)\n\
                    fun invalid(own local: Local): Unit {\n\
                        val sent = dispatch(move { val read = local })\n\
                    }\n\
                    fun empty(): Unit { val sent = dispatch(move {}) }\n\
                    fun opaque(own callback: move () -> Unit): Unit {\n\
                        val sent = dispatch(callback)\n\
                    }";
    let (_, _, _, typed, owned) = analyzed_with(external, cross_thread_environments());
    assert_eq!(codes(&owned), ["L0139", "L0139"]);
    assert!(
        typed
            .calls()
            .iter()
            .filter(|call| !call.arguments().is_empty())
            .all(|call| call.arguments()[0].crosses_thread())
    );

    let source_same_name = "class Local(val callback: () -> Unit)\n\
                            fun dispatch(own callback: move () -> Unit): Unit {}\n\
                            fun ordinary(own local: Local): Unit {\n\
                                val sent = dispatch(move { val read = local })\n\
                            }";
    checked(source_same_name);
}

#[test]
fn checked_in_phase3_closure_fixtures_execute_pass_and_fail_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase3");
    for (directory, should_pass) in [("closure-pass", true), ("closure-fail", false)] {
        let files = fs::read_dir(root.join(directory))
            .expect("closure fixture directory")
            .map(|entry| entry.expect("closure fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1, "zero or unexpected {directory} fixtures");
        for path in files {
            let text = fs::read_to_string(&path).expect("UTF-8 closure fixture");
            let owned = analyzed(&text).4;
            if should_pass {
                assert!(owned.diagnostics().is_empty(), "{path:?}");
                continue;
            }
            let expected =
                fs::read_to_string(path.with_extension("diag")).expect("diagnostic sidecar");
            let actual = owned
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    let span = diagnostic.primary_span();
                    format!("{}\t{}\t{}", diagnostic.code(), span.start(), span.end())
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            assert_eq!(actual, expected, "{path:?}");
        }
    }
}

#[test]
fn ordinary_expected_move_literals_preserve_owned_capture_and_asap_drop() {
    let text = "fun apply(callback: (Int) -> Boolean): Unit {}\n\
                fun make(own returnedLabel: String): (Int) -> Boolean = move { returnedIndex -> returnedLabel == \"return\" && returnedIndex == 0 }\n\
                fun use(own localLabel: String, own argumentLabel: String): Unit {\n\
                    val contextual: (Int) -> Boolean = move { localIndex -> localLabel == \"local\" && localIndex == 0 }\n\
                    apply(contextual)\n\
                    apply(move { argumentIndex -> argumentLabel == \"argument\" && argumentIndex == 0 })\n\
                }";
    let (sources, parsed, names, typed, owned) = checked(text);
    let lambdas = lambdas(&parsed);
    assert_eq!(lambdas.len(), 3);
    assert_eq!(
        owned.loans().len(),
        2,
        "only synchronous callback arguments form loans; owned capture does not borrow its source"
    );
    for (lambda, name) in
        lambdas
            .iter()
            .copied()
            .zip(["returnedLabel", "localLabel", "argumentLabel"])
    {
        let source = symbol(&names, name);
        let captures = owned.captures_of(lambda).collect::<Vec<_>>();
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].source(), ClosureCaptureSource::Symbol(source));
        assert_eq!(captures[0].mode(), ClosureCaptureMode::Owned);
        assert_eq!(captures[0].effect(), ClosureCaptureEffect::Move);
        assert_eq!(
            typed.types().get(captures[0].ty()),
            Some(&lang_frontend::type_checking::TypeKind::Builtin(
                BuiltinType::String
            ))
        );
        assert_eq!(
            typed.copyability(captures[0].ty()),
            Some(lang_frontend::type_checking::Copyability::MoveOnly)
        );
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == DropTarget::Named(source)),
            "moving a String capture removes its source binding's drop obligation"
        );
        let capture_drops = owned
            .drops()
            .iter()
            .filter(|drop| {
                matches!(drop.target(),
            DropTarget::Captured { closure, source: ClosureCaptureSource::Symbol(captured), .. }
                if closure == lambda && captured == source)
            })
            .collect::<Vec<_>>();
        if name == "returnedLabel" {
            assert!(
                capture_drops.is_empty(),
                "returned closure transfers its environment to the caller"
            );
        } else {
            assert_eq!(
                capture_drops.len(),
                1,
                "each captured String drops once with its closure"
            );
            let DropPoint::CallReturn(call) = capture_drops[0].point() else {
                panic!("initializer closure must drop at its final synchronous call return");
            };
            let call_text = sources
                .slice(parsed.ast().expressions().get(call).expect("call").span())
                .expect("call text");
            let expected_call = if name == "localLabel" {
                "apply(contextual)"
            } else {
                "apply(move { argumentIndex -> argumentLabel == \"argument\" && argumentIndex == 0 })"
            };
            assert_eq!(
                call_text, expected_call,
                "capture cleanup belongs to its own final use"
            );
            let loan = owned
                .loans()
                .iter()
                .find(|loan| loan.call() == call)
                .expect("callback borrow");
            assert_eq!(loan.kind(), LoanKind::Shared);
            if name == "localLabel" {
                assert!(
                    matches!(loan.target(), LoanTarget::Place(place) if place.root() == symbol(&names, "contextual"))
                );
            } else {
                assert_eq!(loan.target(), &LoanTarget::Temporary(lambda));
            }
            assert_eq!(
                owned.loans_ending_at(call).count(),
                1,
                "callback loan ends before ASAP environment cleanup"
            );
        }
    }
}

#[test]
fn ordinary_expected_lambda_keeps_capture_escape_and_ownership_errors() {
    for (text, expected) in [
        (
            "fun invalid(label: String): (Int) -> Boolean = { index -> label == \"shared\" && index == 0 }",
            "L0137",
        ),
        (
            "fun invalid(label: String): (Int) -> Boolean = move { index -> label == \"borrowed\" && index == 0 }",
            "L0138",
        ),
        (
            "fun invalid(own label: String): Unit { val f: (Int) -> Boolean = move { index -> label == \"owned\" && index == 0 }\nval after = label == \"after\" }",
            "L0131",
        ),
    ] {
        let (_, _, _, _, owned) = analyzed(text);
        assert_eq!(codes(&owned), [expected], "{text}");
        assert!(
            owned.captures().is_empty(),
            "ownership errors suppress executable capture facts"
        );
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
    }
}
