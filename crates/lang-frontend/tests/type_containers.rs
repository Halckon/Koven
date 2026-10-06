//! SPEC-0023 顺序容器类型、核心构造与 element-place 测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{NameEnvironment, NameResolution, resolve_names},
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        BuiltinType, ContainerConstructionKind, Copyability, ExpressionCategory, IntrinsicCallable,
        IntrinsicTypeConstructor, ParameterMode, SequentialContainerKind, TypeEnvironment,
        TypeKind, TypedFile, check_types,
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
fn deferred_callable_reference_element_does_not_become_a_false_storable_error() {
    let text = "fun identity(input: Int): Int = input\n\
                fun use(): Unit {\n\
                    val reference = ::identity\n\
                    val pending = listOf(reference(1))\n\
                }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.container_constructions().is_empty());
}

#[test]
fn generic_callable_infers_from_nested_intrinsic_container_type() {
    let text = "fun <T> first(input: List<T>): T\n\
                fun use(values: List<Int>): Unit { val selected = first(values) }";
    let (_, _, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let call = typed.calls().last().expect("generic source call");
    assert!(matches!(
        typed.types().get(call.return_type()),
        Some(TypeKind::Builtin(BuiltinType::Int))
    ));
}

#[test]
fn container_size_publishes_receiver_identity_type_and_span() {
    let (sources, parsed, _, typed) = checked(
        "fun sizes(array: Array<Int>, list: List<Int>, mutable: MutableList<Int>): Unit {\n\
             val a = array.size\n\
             val b = (list).size\n\
             val c = mutable.size\n\
             val d = listOf(1).size\n\
         }",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_sizes().len(), 4);
    for descriptor in typed.container_sizes() {
        assert_eq!(
            typed.container_size(descriptor.expression()),
            Some(*descriptor)
        );
        assert_eq!(
            typed.expression_type(descriptor.receiver()),
            Some(descriptor.container_type())
        );
        assert_eq!(
            typed.expression_type(descriptor.expression()),
            Some(descriptor.result_type())
        );
        assert_eq!(
            typed.types().get(descriptor.result_type()),
            Some(&TypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(
            typed.expression_category(descriptor.expression()),
            Some(ExpressionCategory::Temporary)
        );
        assert_eq!(
            parsed
                .ast()
                .expressions()
                .get(descriptor.expression())
                .unwrap()
                .span(),
            descriptor.span()
        );
        assert!(sources.slice(descriptor.span()).unwrap().ends_with(".size"));
    }
    assert_eq!(
        typed.container_sizes()[0].container(),
        SequentialContainerKind::Array
    );
    assert_eq!(
        typed.container_sizes()[1].container(),
        SequentialContainerKind::List
    );
    assert_eq!(
        typed.container_sizes()[2].container(),
        SequentialContainerKind::MutableList
    );
}

#[test]
fn mutable_list_add_member_is_typed() {
    let text = "fun append(own list: MutableList<Int>): Unit { list.add(42) }";
    let (_sources, _parsed, _names, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_appends().len(), 1);
    let append = typed.container_appends()[0];
    assert_eq!(
        typed.types().get(append.result_type()),
        Some(&lang_frontend::type_checking::TypeKind::Builtin(
            lang_frontend::type_checking::BuiltinType::Unit
        ))
    );
}

#[test]
fn container_add_rejects_array_and_list() {
    let text = "fun bad(array: Array<Int>, list: List<Int>): Unit {\n\
                    array.add(1)\n\
                    list.add(2)\n\
                }";
    let (_sources, _parsed, _names, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0130", "L0130"]);
}

#[test]
fn container_add_rejects_property_access() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    val f = list.add\n\
                }";
    let (_sources, _parsed, _names, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0130"]);
}

#[test]
fn container_add_rejects_mismatched_element_type() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.add(\"hello\")\n\
                }";
    let (_sources, _parsed, _names, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0084"]);
}


