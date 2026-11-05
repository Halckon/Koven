//! SPEC-0022 conditional Copyable, inline layout, intrinsic Box, and destructuring tests.

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{
        CompilationUnitNames, NameEnvironment, SourceUnitInput, SourceUnitKey, UnitSymbolId,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{ParsedFile, Statement},
    source::SourceMap,
    type_checking::{
        BuiltinType, Capability, CompilationUnitTypes, Copyability, DestructuringMode,
        IntrinsicTypeConstructor, TypeEnvironment, UnitStatementId, UnitTypeKind,
        check_compilation_unit_types,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

#[path = "type_copyability/box_signatures.rs"]
mod box_signatures;

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

fn checked_entries(
    text: &str,
    with_box: bool,
) -> (
    SourceMap,
    ParsedFile,
    [(CompilationUnitNames, CompilationUnitTypes); 2],
) {
    let (sources, parsed) = parse(text);
    let (environment, types) = environments(with_box);
    let entries = [false, true].map(|rooted| {
        let input = if rooted {
            SourceUnitInput::new("root", "copyability.ko", parsed.source_id(), &parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), parsed.source_id());
        assert!(match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == "copyability.ko",
            _ => false,
        });
        let resolution = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(
            resolution.diagnostics().is_empty(),
            "{:?}",
            resolution.diagnostics()
        );
        let names = resolution.validate().expect("valid unit names");
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
        (names.into_names(), typed)
    });
    (sources, parsed, entries)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn symbol(
    typed: &CompilationUnitTypes,
    resolution: &CompilationUnitNames,
    name: &str,
) -> UnitSymbolId {
    let [source] = resolution.source_units() else {
        panic!("one original names source")
    };
    let local = source
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .unwrap_or_else(|| panic!("missing symbol {name}"))
        .id();
    let declarations = typed.signatures().declarations();
    typed
        .body_symbol_types()
        .keys()
        .copied()
        .chain(declarations.iter().map(|declaration| declaration.symbol()))
        .chain(
            declarations
                .iter()
                .filter_map(|declaration| declaration.callable())
                .flat_map(|callable| callable.parameters())
                .filter_map(|parameter| parameter.symbol()),
        )
        .find(|symbol| symbol.source_unit() == source.source_unit() && symbol.symbol() == local)
        .unwrap_or_else(|| panic!("missing qualified symbol {name}"))
}

fn copyability(
    typed: &CompilationUnitTypes,
    resolution: &CompilationUnitNames,
    name: &str,
) -> Copyability {
    let ty = typed
        .symbol_type(symbol(typed, resolution, name))
        .unwrap_or_else(|| panic!("missing type for {name}"));
    assert!(
        typed.types().get(ty).is_some(),
        "missing type entry for {name}"
    );
    typed.copyability(ty)
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
    let (_, _, entries) = checked_entries(text, true);
    for (resolution, typed) in entries {
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
}

#[test]
fn copyable_bounds_use_l0115_and_keep_interface_bounds_unchanged() {
    let text = "value class Holder<T : Copyable>(val payload: T)\n\
                enum class Plain { A, B }\n\
                enum class Resource { Item(payload: String) }\n\
                class Ref\n\
                fun valid(a: Holder<Int>, b: Holder<Plain>): Unit {}\n\
                fun invalid(a: Holder<Ref>, b: Holder<Resource>): Unit {}";
    let (sources, _, entries) = checked_entries(text, false);
    for (_resolution, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0115", "L0115"]);
        let primaries = typed
            .diagnostics()
            .iter()
            .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
            .collect::<Vec<_>>();
        assert_eq!(primaries, ["Ref", "Resource"]);
    }
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
    let (sources, _, entries) = checked_entries(text, false);
    for (resolution, typed) in entries {
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
}

#[test]
fn complex_inline_component_reports_once_and_suppresses_copyable_cascade() {
    let text = "value class Holder<T : Copyable>(val payload: T)\n\
                value class A(val b: B)\n\
                value class B(val a: A, val c: C)\n\
                value class C(val b: B)\n\
                fun use(held: Holder<A>): Unit {}";
    let (_, _, entries) = checked_entries(text, false);
    for (resolution, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0116"]);
        for name in ["A", "B", "C", "held"] {
            assert_eq!(
                copyability(&typed, &resolution, name),
                Copyability::Error,
                "{name}"
            );
        }
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
    let (sources, _, entries) = checked_entries(text, true);
    for (resolution, typed) in entries {
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
                .symbol_type(symbol(&typed, &resolution, "a"))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Box,
                ..
            })
        ));
    }
}

#[test]
fn a_source_class_named_box_never_gains_intrinsic_rules() {
    let text = "class Box<T>\nfun use(box: Box<Int>): Unit {}";
    let (_, _, entries) = checked_entries(text, true);
    for (resolution, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(matches!(
            typed
                .symbol_type(symbol(&typed, &resolution, "box"))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Nominal { .. })
        ));
    }
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
    let (_, parsed, entries) = checked_entries(text, false);
    for (resolution, typed) in entries {
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
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
        assert!(matches!(
            component_kinds[1],
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        ));
        assert_eq!(
            typed.destructurings()[0]
                .components()
                .iter()
                .map(|component| component.symbol())
                .collect::<Vec<_>>(),
            [
                symbol(&typed, &resolution, "a"),
                symbol(&typed, &resolution, "b")
            ]
        );
        let statements = parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Statement::LocalDestructuring { .. }).then_some(id)
            })
            .map(|id| UnitStatementId::new(resolution.source_units()[0].source_unit(), id))
            .collect::<Vec<_>>();
        assert_eq!(typed.destructurings()[0].statement(), statements[0]);
        assert_eq!(typed.destructurings()[1].statement(), statements[1]);
        assert!(typed.destructuring(statements[2]).is_none());
        for name in ["e", "f"] {
            assert!(matches!(
                typed
                    .symbol_type(symbol(&typed, &resolution, name))
                    .and_then(|ty| typed.types().get(ty)),
                Some(UnitTypeKind::Deferred(_))
            ));
        }
    }
}

#[test]
fn destructuring_arity_uses_l0118_and_keeps_prefix_types_only_for_recovery() {
    let text = "value class Pair(val first: Int, val second: String)\n\
                fun split(pair: Pair): Unit {\n\
                    val (one) = pair\n\
                    val (x, y, extra) = pair\n\
                }";
    let (sources, _, entries) = checked_entries(text, false);
    for (resolution, typed) in entries {
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
                .symbol_type(symbol(&typed, &resolution, "one"))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
        assert!(matches!(
            typed
                .symbol_type(symbol(&typed, &resolution, "extra"))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Error)
        ));
    }
}
