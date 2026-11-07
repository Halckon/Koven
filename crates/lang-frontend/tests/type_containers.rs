//! SPEC-0023 顺序容器类型、核心构造与 element-place 测试。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{
        NameEnvironment, SourceUnitId, SourceUnitInput, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{
        BuiltinType, CompilationUnitTypes, ContainerConstructionKind, Copyability,
        ExpressionCategory, IntrinsicCallable, IntrinsicTypeConstructor, ParameterMode,
        SequentialContainerKind, TypeEnvironment, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

#[path = "type_containers/mutable_unit.rs"]
mod mutable_unit;

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

fn checked_entries(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    [(SourceUnitId, CompilationUnitTypes); 2],
) {
    let (sources, parsed) = parsed(text);
    let source = parsed.source_id();
    let (environment, types) = environments();
    let entries = [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "containers.ko", source, &parsed),
    ]
    .map(|input| {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [entry] = index.source_units() else {
            panic!("one original source");
        };
        assert_eq!(entry.source_id(), source);
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).expect("names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
        (entry.id(), typed)
    });
    (sources, parsed, entries)
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
    let (_, parsed, entries) = checked_entries(text);
    for (entry, typed) in entries {
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
                    == Copyability::MoveOnly)
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
            typed.expression_category(UnitExpressionId::new(entry, size)),
            Some(ExpressionCategory::Temporary)
        );
    }
}

#[test]
fn non_storable_and_wrong_arity_container_types_are_rejected() {
    let text = "interface Shape\n\
                fun invalid(\n\
                    never: List<Nothing>, top: Array<Any>, dynamic: List<Shape>,\n\
                    arity: MutableList<Int, Long>, callable: List<(Nothing) -> Nothing>\n\
                ): Unit {}";
    for (_, typed) in checked_entries(text).2 {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0125", "L0125", "L0094", "L0091"]
        );
    }
}

#[test]
fn non_storable_sequential_signature_elements_keep_one_error_at_the_argument() {
    let text = "value class Wrapped<T>(val value: T)\n\
                fun reject(a: List<Nothing>, b: Array<Any>, c: MutableList<Nothing?>,\n\
                           d: List<List<Any>>, e: Array<Wrapped<Any>>): Unit {}\n\
                fun <T> keep(x: List<T>, f: Array<(Nothing) -> Nothing>): Unit {}";
    let (sources, parsed, entries) = checked_entries(text);
    for (entry, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0125"; 5]);
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).unwrap())
                .collect::<Vec<_>>(),
            ["Nothing", "Any", "Nothing?", "Any", "Wrapped<Any>"]
        );
        assert_eq!(typed.signatures().diagnostics(), typed.diagnostics());
        assert!(typed.body_diagnostics().is_empty());
        let invalid = parsed
            .ast()
            .type_refs()
            .iter()
            .filter(|(_, node)| {
                matches!(
                    sources.slice(node.span()),
                    Ok("List<Nothing>"
                        | "Array<Any>"
                        | "MutableList<Nothing?>"
                        | "List<Any>"
                        | "List<List<Any>>"
                        | "Array<Wrapped<Any>>")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(invalid.len(), 6);
        for (id, _) in invalid {
            let ty = typed
                .type_ref_type(lang_frontend::type_checking::UnitTypeRefId::new(entry, id))
                .expect("signature type ref");
            assert!(matches!(
                typed.types().get(ty),
                Some(UnitTypeKind::Intrinsic { .. })
            ));
        }
        assert!(typed.validate().is_err());
    }
}

#[test]
fn non_storable_sequential_signatures_do_not_collapse_overload_shapes() {
    let text = "fun select(value: List<Nothing>): Unit {}\n\
                fun select(value: Array<Any>): Unit {}\n\
                fun select(value: Int): Int = value\n\
                fun entry(): Int = select(1)";
    for (_, typed) in checked_entries(text).2 {
        assert_eq!(codes(typed.diagnostics()), ["L0125"; 2]);
        assert!(typed.body_diagnostics().is_empty());
        let [call] = typed.calls() else {
            panic!("one call to the valid overload");
        };
        assert_eq!(
            typed.types().get(call.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert!(typed.validate().is_err());
    }
}

#[test]
fn sequential_elements_inside_static_bounds_still_require_runtime_types() {
    let text = "interface Shape<T>\n\
                fun <T: Shape<List<Shape<Int>>>> reject(): Unit {}\n\
                fun nested(arg: List<Shape<Nothing>>): Unit {}\n\
                fun <T: Shape<List<Int>>> keep(): Unit {}";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0094"; 2]);
        for (diagnostic, primary) in typed
            .diagnostics()
            .iter()
            .zip(["Shape<Int>", "Shape<Nothing>"])
        {
            assert_eq!(sources.slice(diagnostic.primary_span()), Ok(primary));
            let [DiagnosticDetail::Label(label)] = diagnostic.details() else {
                panic!("one interface declaration label: {diagnostic:?}");
            };
            assert_eq!(sources.slice(label.span()), Ok("Shape"));
        }
        assert!(typed.validate().is_err());
    }
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
    for (_, typed) in checked_entries(text).2 {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0126", "L0126", "L0084", "L0084", "L0127"]
        );
    }
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
    for (_, typed) in checked_entries(text).2 {
        assert_eq!(
            codes(typed.diagnostics()),
            [
                "L0128", "L0129", "L0129", "L0129", "L0130", "L0130", "L0122", "L0122", "L0122"
            ]
        );
    }
}

#[test]
fn source_names_cannot_impersonate_intrinsic_containers_or_constructors() {
    let text = "class List<T>\n\
                fun listOf(input: Int): Int = input\n\
                fun inspect(sourceList: List<Int>): Int = listOf(1)";
    for (_, typed) in checked_entries(text).2 {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.container_constructions().is_empty());
    }
}

#[test]
fn deferred_callable_reference_element_does_not_become_a_false_storable_error() {
    let text = "fun identity(input: Int): Int = input\n\
                fun use(): Unit {\n\
                    val reference = ::identity\n\
                    val pending = listOf(reference(1))\n\
                }";
    for (_, typed) in checked_entries(text).2 {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.container_constructions().is_empty());
    }
}

#[test]
fn generic_callable_infers_from_nested_intrinsic_container_type() {
    let text = "fun <T> first(input: List<T>): T\n\
                fun use(values: List<Int>): Unit { val selected = first(values) }";
    for (_, typed) in checked_entries(text).2 {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let call = typed.calls().last().expect("generic source call");
        assert!(matches!(
            typed.types().get(call.return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
}

#[test]
fn container_size_publishes_receiver_identity_type_and_span() {
    let (sources, parsed, entries) = checked_entries(
        "fun sizes(array: Array<Int>, list: List<Int>, mutable: MutableList<Int>): Unit {\n\
             val a = array.size\n\
             val b = (list).size\n\
             val c = mutable.size\n\
             val d = listOf(1).size\n\
         }",
    );
    for (entry, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.container_sizes().len(), 4);
        for descriptor in typed.container_sizes() {
            assert_eq!(descriptor.expression().source_unit(), entry);
            assert_eq!(descriptor.receiver().source_unit(), entry);
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
                Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            );
            assert_eq!(
                typed.expression_category(descriptor.expression()),
                Some(ExpressionCategory::Temporary)
            );
            assert_eq!(
                parsed
                    .ast()
                    .expressions()
                    .get(descriptor.expression().expression())
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
}

#[test]
fn mutable_list_add_member_is_typed() {
    let text = "fun append(own list: MutableList<Int>): Unit { list.add(42) }";
    mutable_unit::check_each(text, &[], |typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.container_appends().len(), 1);
        let append = typed.container_appends()[0];
        assert_eq!(
            typed.types().get(append.result_type()),
            Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Unit
            ))
        );
    });
}

#[test]
fn container_add_rejects_array_and_list() {
    let text = "fun bad(array: Array<Int>, list: List<Int>): Unit {\n\
                    array.add(1)\n\
                    list.add(2)\n\
                }";
    mutable_unit::check_each(text, &["L0130", "L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130", "L0130"]);
    });
}

#[test]
fn container_add_rejects_property_access() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    val f = list.add\n\
                }";
    mutable_unit::check_each(text, &["L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130"]);
    });
}

#[test]
fn container_add_rejects_mismatched_element_type() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.add(\"hello\")\n\
                }";
    mutable_unit::check_each(text, &["L0084"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    });
}

#[test]
fn mutable_list_clear_member_is_typed() {
    let text = "fun clearList(own list: MutableList<Int>): Unit { list.clear() }";
    mutable_unit::check_each(text, &[], |typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.container_clears().len(), 1);
        let clear = typed.container_clears()[0];
        assert_eq!(
            typed.types().get(clear.result_type()),
            Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Unit
            ))
        );
    });
}

#[test]
fn container_clear_rejects_array_and_list() {
    let text = "fun bad(array: Array<Int>, list: List<Int>): Unit {\n\
                    array.clear()\n\
                    list.clear()\n\
                }";
    mutable_unit::check_each(text, &["L0130", "L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130", "L0130"]);
    });
}

#[test]
fn container_clear_rejects_arguments() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.clear(1)\n\
                }";
    mutable_unit::check_each(text, &["L0121"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0121"]);
    });
}

#[test]
fn mutable_list_remove_at_member_is_typed() {
    let text = "fun pop(own list: MutableList<Int>): Int { return list.removeAt(0) }";
    mutable_unit::check_each(text, &[], |typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.container_remove_ats().len(), 1);
        let remove_at = typed.container_remove_ats()[0];
        assert_eq!(
            typed.types().get(remove_at.result_type()),
            Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Int
            ))
        );
        assert_eq!(
            typed.container_remove_at(remove_at.expression()),
            Some(remove_at)
        );
    });
}

#[test]
fn container_remove_at_rejects_array_and_list() {
    let text = "fun bad(array: Array<Int>, list: List<Int>): Unit {\n\
                    array.removeAt(0)\n\
                    list.removeAt(0)\n\
                }";
    mutable_unit::check_each(text, &["L0130", "L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130", "L0130"]);
    });
}

#[test]
fn container_remove_at_rejects_property_access() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    val f = list.removeAt\n\
                }";
    mutable_unit::check_each(text, &["L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130"]);
    });
}

#[test]
fn container_remove_at_rejects_mismatched_index_and_arity() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.removeAt()\n\
                    list.removeAt(0, 1)\n\
                    list.removeAt(\"zero\")\n\
                }";
    mutable_unit::check_each(text, &["L0121", "L0121", "L0084"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0121", "L0121", "L0084"]);
    });
}

#[test]
fn mutable_list_remove_last_member_is_typed() {
    let text = "fun pop(own list: MutableList<Int>): Int { return list.removeLast() }";
    mutable_unit::check_each(text, &[], |typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.container_remove_lasts().len(), 1);
        let remove_last = typed.container_remove_lasts()[0];
        assert_eq!(
            typed.types().get(remove_last.result_type()),
            Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Int
            ))
        );
        assert_eq!(
            typed.container_remove_last(remove_last.expression()),
            Some(remove_last)
        );
    });
}

#[test]
fn container_remove_last_rejects_array_and_list() {
    let text = "fun bad(array: Array<Int>, list: List<Int>): Unit {\n\
                    array.removeLast()\n\
                    list.removeLast()\n\
                }";
    mutable_unit::check_each(text, &["L0130", "L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130", "L0130"]);
    });
}

#[test]
fn container_remove_last_rejects_property_access() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    val f = list.removeLast\n\
                }";
    mutable_unit::check_each(text, &["L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130"]);
    });
}

#[test]
fn container_remove_last_rejects_arguments() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.removeLast(0)\n\
                }";
    mutable_unit::check_each(text, &["L0121"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0121"]);
    });
}

#[test]
fn mutable_list_remove_first_member_is_typed() {
    let text = "fun pop_front(own list: MutableList<Int>): Int { return list.removeFirst() }";
    mutable_unit::check_each(text, &[], |typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.container_remove_firsts().len(), 1);
        let remove_first = typed.container_remove_firsts()[0];
        assert_eq!(
            typed.types().get(remove_first.result_type()),
            Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Int
            ))
        );
        assert_eq!(
            typed.container_remove_first(remove_first.expression()),
            Some(remove_first)
        );
    });
}

#[test]
fn container_remove_first_rejects_array_and_list() {
    let text = "fun bad(array: Array<Int>, list: List<Int>): Unit {\n\
                    array.removeFirst()\n\
                    list.removeFirst()\n\
                }";
    mutable_unit::check_each(text, &["L0130", "L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130", "L0130"]);
    });
}

#[test]
fn container_remove_first_rejects_property_access() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    val f = list.removeFirst\n\
                }";
    mutable_unit::check_each(text, &["L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130"]);
    });
}

#[test]
fn container_remove_first_rejects_arguments() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.removeFirst(0)\n\
                }";
    mutable_unit::check_each(text, &["L0121"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0121"]);
    });
}

#[test]
fn mutable_list_insert_at_member_is_typed() {
    let text = "fun insert(own list: MutableList<Int>): Unit { list.insertAt(0, 42) }";
    mutable_unit::check_each(text, &[], |typed| {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.container_insert_ats().len(), 1);
        let insert_at = typed.container_insert_ats()[0];
        assert_eq!(
            typed.types().get(insert_at.result_type()),
            Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Unit
            ))
        );
        assert_eq!(
            typed.container_insert_at(insert_at.expression()),
            Some(insert_at)
        );
    });
}

#[test]
fn container_insert_at_rejects_array_and_list() {
    let text = "fun bad(array: Array<Int>, list: List<Int>): Unit {\n\
                    array.insertAt(0, 1)\n\
                    list.insertAt(0, 1)\n\
                }";
    mutable_unit::check_each(text, &["L0130", "L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130", "L0130"]);
    });
}

#[test]
fn container_insert_at_rejects_property_access() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    val f = list.insertAt\n\
                }";
    mutable_unit::check_each(text, &["L0130"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0130"]);
    });
}

#[test]
fn container_insert_at_rejects_mismatched_element_type() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.insertAt(0, \"hello\")\n\
                }";
    mutable_unit::check_each(text, &["L0084"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    });
}

#[test]
fn container_insert_at_rejects_mismatched_index_type() {
    let text = "fun bad(own list: MutableList<Int>): Unit {\n\
                    list.insertAt(\"zero\", 42)\n\
                }";
    mutable_unit::check_each(text, &["L0084"], |typed| {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    });
}
