//! SPEC-0023 顺序容器类型、核心构造与 element-place 测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{NameEnvironment, NameResolution, resolve_names},
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        BuiltinType, ContainerConstructionKind, Copyability, ExpressionCategory, IntrinsicCallable,
        IntrinsicTypeConstructor, ParameterMode, SequentialContainerKind, TypeEnvironment,
        TypedFile, check_types,
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
    let source = sources.add_source("containers.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "sequential container type source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (sources, parsed)
}

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BUILTINS.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        )
    });
    let containers = [
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
    let constructors = [
        (
            names.declare_function("arrayOf").expect("arrayOf"),
            IntrinsicCallable::ArrayOf,
        ),
        (
            names.declare_function("listOf").expect("listOf"),
            IntrinsicCallable::ListOf,
        ),
        (
            names
                .declare_function("mutableListOf")
                .expect("mutableListOf"),
            IntrinsicCallable::MutableListOf,
        ),
    ];
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, container) in containers {
        types
            .bind_intrinsic(symbol, container)
            .expect("container binding");
    }
    for (symbol, constructor) in constructors {
        types
            .bind_intrinsic_callable(symbol, constructor)
            .expect("constructor binding");
    }
    (names, types)
}

fn checked(text: &str) -> (SourceMap, ParsedFile, NameResolution, TypedFile) {
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
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

#[test]
fn container_types_and_core_constructions_preserve_element_identity() {
    let text = "fun build(size: Int): Unit {\n\
                    val inferred = listOf(1, 2)\n\
                    val expected: List<Int> = listOf()\n\
                    val nullable: List<Int?> = listOf(null)\n\
                    val explicit = arrayOf<Long>()\n\
                    val initializer: (Int) -> Int = { index -> index }\n\
                    val array = Array<Int>(size, initializer)\n\
                    val list = List<Int>(size, initializer)\n\
                    val mutable = MutableList<Int>()\n\
                    val first: Int = inferred[0]\n\
                    val replaced = (array[0] = 3)\n\
                    val changed = (mutable[0] = 4)\n\
                    val count: Int = list.size\n\
                }";
    let (_, parsed, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_constructions().len(), 7);
    assert_eq!(
        typed
            .container_constructions()
            .iter()
            .map(|descriptor| descriptor.kind())
            .collect::<Vec<_>>(),
        [
            ContainerConstructionKind::ListForm,
            ContainerConstructionKind::ListForm,
            ContainerConstructionKind::ListForm,
            ContainerConstructionKind::ListForm,
            ContainerConstructionKind::RuntimeLength,
            ContainerConstructionKind::RuntimeLength,
            ContainerConstructionKind::EmptyMutableList,
        ]
    );
    assert_eq!(
        typed.container_constructions()[4].parameter_modes(),
        [ParameterMode::Borrow, ParameterMode::Borrow]
    );
    assert!(
        typed
            .container_constructions()
            .iter()
            .all(|descriptor| typed.copyability(descriptor.container_type())
                == Some(Copyability::MoveOnly))
    );
    assert_eq!(typed.element_places().len(), 3);
    assert_eq!(
        typed
            .element_places()
            .iter()
            .map(|place| (place.container(), place.is_mutable()))
            .collect::<Vec<_>>(),
        [
            (SequentialContainerKind::List, false),
            (SequentialContainerKind::Array, true),
            (SequentialContainerKind::MutableList, true),
        ]
    );
    let size = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| match node.payload() {
            Expression::Member { name_span, .. }
                if text.get(name_span.start()..name_span.end()) == Some("size") =>
            {
                Some(id)
            }
            _ => None,
        })
        .expect("size expression");
    assert_eq!(
        typed.expression_category(size),
        Some(ExpressionCategory::Temporary)
    );
}

#[test]
fn non_storable_and_wrong_arity_container_types_are_rejected() {
    let text = "interface Shape\n\
                fun invalid(\n\
                    never: List<Nothing>, top: Array<Any>, dynamic: List<Shape>,\n\
                    arity: MutableList<Int, Long>, callable: List<(Nothing) -> Nothing>\n\
                ): Unit {}";
    let (_, _, _, typed) = checked(text);
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0125", "L0125", "L0094", "L0091"]
    );
}

#[test]
fn construction_inference_and_fixed_contract_fail_loudly() {
    let text = "fun invalid(size: Boolean): Unit {\n\
                    val empty = listOf()\n\
                    val absent = listOf(null)\n\
                    val mixed = listOf(1, true)\n\
                    val initializer: (Int) -> Int = { index -> index }\n\
                    val runtime = Array<Int>(size, initializer)\n\
                    val wrongMutable = MutableList<Int>(1)\n\
                }";
    let (_, _, _, typed) = checked(text);
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0126", "L0126", "L0084", "L0084", "L0127"]
    );
}

#[test]
fn index_mutability_size_and_get_set_follow_the_closed_place_contract() {
    let text = "fun mutate(inout input: Int): Unit {}\n\
                fun invalid(list: List<Int>, array: Array<Int>): Unit {\n\
                    val badIndex = list[true]\n\
                    val replacement = (list[0] = 1)\n\
                    val groupedReplacement = ((list[0]) = 1)\n\
                    val resize = (list.size = 2)\n\
                    val get = list.get(0)\n\
                    val set = list.set(0, 1)\n\
                    val borrowed = mutate(&list[0])\n\
                    val groupedBorrowed = mutate(&(list[0]))\n\
                    val borrowedSize = mutate(&list.size)\n\
                    val changed = mutate(&array[0])\n\
                }";
    let (_, _, _, typed) = checked(text);
    assert_eq!(
        codes(typed.diagnostics()),
        [
            "L0128", "L0129", "L0129", "L0129", "L0130", "L0130", "L0122", "L0122", "L0122"
        ]
    );
}

#[test]
fn source_names_cannot_impersonate_intrinsic_containers_or_constructors() {
    let text = "class List<T>\n\
                fun listOf(input: Int): Int = input\n\
                fun inspect(sourceList: List<Int>): Int = listOf(1)";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.container_constructions().is_empty());
}

#[test]
fn deferred_generic_element_does_not_become_a_false_storable_error() {
    let text = "fun <T> identity(input: T): T = input\n\
                fun use(): Unit { val pending = listOf(identity(1)) }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.container_constructions().is_empty());
}
