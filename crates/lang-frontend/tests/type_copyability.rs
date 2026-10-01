//! SPEC-0022 conditional Copyable, inline layout, intrinsic Box, and destructuring tests.

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{NameEnvironment, NameResolution, SymbolId, resolve_names},
    parser::{ParsedFile, Statement},
    source::SourceMap,
    type_checking::{
        BuiltinType, Capability, Copyability, DestructuringMode, IntrinsicTypeConstructor,
        TypeEnvironment, TypeKind, TypedFile, check_types,
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

fn parse(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("copyability.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "copyability type source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (sources, parsed)
}

fn environments(with_box: bool) -> (NameEnvironment, TypeEnvironment) {
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
    let box_symbol = with_box.then(|| names.declare_type("Box").expect("Box"));
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
    if let Some(symbol) = box_symbol {
        types
            .bind_intrinsic(symbol, IntrinsicTypeConstructor::Box)
            .expect("Box binding");
    }
    (names, types)
}

fn checked(text: &str, with_box: bool) -> (SourceMap, ParsedFile, NameResolution, TypedFile) {
    let (sources, parsed) = parse(text);
    let (names, types) = environments(with_box);
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    (sources, parsed, resolution, typed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn symbol(resolution: &NameResolution, name: &str) -> SymbolId {
    resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .unwrap_or_else(|| panic!("missing symbol {name}"))
        .id()
}

fn copyability(typed: &TypedFile, resolution: &NameResolution, name: &str) -> Copyability {
    let ty = typed
        .symbol_type(symbol(resolution, name))
        .unwrap_or_else(|| panic!("missing type for {name}"));
    typed
        .copyability(ty)
        .unwrap_or_else(|| panic!("missing copyability for {name}"))
}

#[test]
fn structural_copyability_covers_builtins_generics_enums_and_type_parameters() {
    let text = "value class Pair<A, B>(val first: A, val second: B)\n\
                enum class Flag { On, Off }\n\
                enum class Maybe<T> { None, Some(payload: T) }\n\
                class Ref\n\
                fun <C : Copyable, U> inspect(\n\
                    scalar: Int, never: Nothing, nullable: Int?, text: String, top: Any,\n\
                    pairCopy: Pair<Int, Int>, pairMove: Pair<Int, String>,\n\
                    flag: Flag, enumCopy: Maybe<Int>, enumMove: Maybe<String>,\n\
                    bounded: C, unbounded: U, reference: Ref\n\
                ): Unit {}";
    let (_, _, resolution, typed) = checked(text, true);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    for name in [
        "scalar", "never", "nullable", "pairCopy", "flag", "enumCopy", "bounded",
    ] {
        assert_eq!(
            copyability(&typed, &resolution, name),
            Copyability::Copyable,
            "{name}"
        );
    }
    for name in [
        "text",
        "top",
        "pairMove",
        "enumMove",
        "unbounded",
        "reference",
    ] {
        assert_eq!(
            copyability(&typed, &resolution, name),
            Copyability::MoveOnly,
            "{name}"
        );
    }
}

#[test]
fn copyable_bounds_use_l0115_and_keep_interface_bounds_unchanged() {
    let text = "value class Holder<T : Copyable>(val payload: T)\n\
                enum class Plain { A, B }\n\
                enum class Resource { Item(payload: String) }\n\
                class Ref\n\
                fun valid(a: Holder<Int>, b: Holder<Plain>): Unit {}\n\
                fun invalid(a: Holder<Ref>, b: Holder<Resource>): Unit {}";
    let (sources, _, _, typed) = checked(text, false);
    assert_eq!(codes(typed.diagnostics()), ["L0115", "L0115"]);
    let primaries = typed
        .diagnostics()
        .iter()
        .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
        .collect::<Vec<_>>();
    assert_eq!(primaries, ["Ref", "Resource"]);
}

#[test]
fn inline_layout_reports_one_stable_cycle_per_component() {
    let text = "value class Direct(val next: Direct)\n\
                value class Left(val right: Right)\n\
                value class Right(val left: Left)\n\
                enum class Recursive { Next(payload: Recursive) }\n\
                value class Pair<A, B>(val first: A, val second: B)\n\
                value class Growing<T>(val next: Growing<Pair<T, T>>)\n\
                class Node(var next: Node?)\n\
                value class Finite(val node: Node)";
    let (sources, _, resolution, typed) = checked(text, false);
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0116", "L0116", "L0116", "L0116"]
    );
    let primaries = typed
        .diagnostics()
        .iter()
        .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
        .collect::<Vec<_>>();
    assert_eq!(
        primaries,
        ["Direct", "Left", "Recursive", "Growing<Pair<T, T>>"]
    );
    for name in ["Direct", "Left", "Right", "Recursive", "Growing"] {
        assert_eq!(
            copyability(&typed, &resolution, name),
            Copyability::Error,
            "{name}"
        );
    }
    assert_eq!(
        copyability(&typed, &resolution, "Finite"),
        Copyability::MoveOnly
    );
}

#[test]
fn complex_inline_component_reports_once_and_suppresses_copyable_cascade() {
    let text = "value class Holder<T : Copyable>(val payload: T)\n\
                value class A(val b: B)\n\
                value class B(val a: A, val c: C)\n\
                value class C(val b: B)\n\
                fun use(held: Holder<A>): Unit {}";
    let (_, _, resolution, typed) = checked(text, false);
    assert_eq!(codes(typed.diagnostics()), ["L0116"]);
    for name in ["A", "B", "C", "held"] {
        assert_eq!(
            copyability(&typed, &resolution, name),
            Copyability::Error,
            "{name}"
        );
    }
}

#[test]
fn intrinsic_box_accepts_concrete_value_and_enum_classes_and_breaks_layout_cycles() {
    let text = "value class Link(val next: Box<Link>?)\n\
                value class Point(val x: Int)\n\
                class Ref\n\
                class Heap\n\
                enum class Choice { A }\n\
                enum class Expr { Num(value: Int), Add(left: Box<Expr>, right: Box<Expr>) }\n\
                fun valid(a: Box<Point>, b: Box<Choice>, recursive: Link, expr: Expr): Unit {}\n\
                fun <T : Copyable> invalidGeneric(a: Box<T>): Unit {}\n\
                fun invalid(a: Box<Ref>, b: Box<Heap>, c: Box<Int>, d: Box, e: Box<Point, Point>): Unit {}";
    let (sources, _, resolution, typed) = checked(text, true);
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0117", "L0117", "L0117", "L0117", "L0091", "L0091"]
    );
    let mut primaries = typed
        .diagnostics()
        .iter()
        .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
        .collect::<Vec<_>>();
    primaries.sort_unstable();
    assert_eq!(primaries, ["Box", "Box", "Heap", "Int", "Ref", "T"]);
    assert_eq!(
        copyability(&typed, &resolution, "recursive"),
        Copyability::MoveOnly
    );
    assert!(matches!(
        typed
            .symbol_type(symbol(&resolution, "a"))
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Box,
            ..
        })
    ));
}

#[test]
fn a_source_class_named_box_never_gains_intrinsic_rules() {
    let text = "class Box<T>\nfun use(box: Box<Int>): Unit {}";
    let (_, _, resolution, typed) = checked(text, true);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(matches!(
        typed
            .symbol_type(symbol(&resolution, "box"))
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Nominal { .. })
    ));
}

#[test]
fn value_class_destructuring_records_copy_and_consume_after_forward_declaration() {
    let text = "fun split(copy: Pair<Int, Int>, moved: Pair<Int, String>, ref: Ref): Unit {\n\
                    val (a, b) = copy\n\
                    val (c, d) = moved\n\
                    val (e, f) = ref\n\
                }\n\
                value class Pair<A, B>(val first: A, val second: B)\n\
                class Ref";
    let (_, parsed, resolution, typed) = checked(text, false);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.destructurings().len(), 2);
    assert_eq!(typed.destructurings()[0].mode(), DestructuringMode::Copy);
    assert_eq!(typed.destructurings()[1].mode(), DestructuringMode::Consume);
    let component_kinds = typed.destructurings()[1]
        .components()
        .iter()
        .map(|component| typed.types().get(component.ty()))
        .collect::<Vec<_>>();
    assert!(matches!(
        component_kinds[0],
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        component_kinds[1],
        Some(TypeKind::Builtin(BuiltinType::String))
    ));
    assert_eq!(
        typed.destructurings()[0]
            .components()
            .iter()
            .map(|component| component.symbol())
            .collect::<Vec<_>>(),
        [symbol(&resolution, "a"), symbol(&resolution, "b")]
    );
    let statements = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Statement::LocalDestructuring { .. }).then_some(id)
        })
        .collect::<Vec<_>>();
    assert_eq!(typed.destructurings()[0].statement(), statements[0]);
    assert_eq!(typed.destructurings()[1].statement(), statements[1]);
    assert!(typed.destructuring(statements[2]).is_none());
    for name in ["e", "f"] {
        assert!(matches!(
            typed
                .symbol_type(symbol(&resolution, name))
                .and_then(|ty| typed.types().get(ty)),
            Some(TypeKind::Deferred(_))
        ));
    }
}

#[test]
fn destructuring_arity_uses_l0118_and_keeps_prefix_types_only_for_recovery() {
    let text = "value class Pair(val first: Int, val second: String)\n\
                fun split(pair: Pair): Unit {\n\
                    val (one) = pair\n\
                    val (x, y, extra) = pair\n\
                }";
    let (sources, _, resolution, typed) = checked(text, false);
    assert_eq!(codes(typed.diagnostics()), ["L0118", "L0118"]);
    assert_eq!(typed.destructurings().len(), 0);
    let primaries = typed
        .diagnostics()
        .iter()
        .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
        .collect::<Vec<_>>();
    assert_eq!(primaries, ["(one)", "(x, y, extra)"]);
    assert!(matches!(
        typed
            .symbol_type(symbol(&resolution, "one"))
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        typed
            .symbol_type(symbol(&resolution, "extra"))
            .and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Error)
    ));
}
