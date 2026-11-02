//! SPEC-0028 条件复制、消费式解构与结构分量移动测试。

use std::{fs, path::Path};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{
        CompilationUnitNames, NameEnvironment, SourceUnitInput, UnitSymbolId,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    source::SourceMap,
    type_checking::{
        BuiltinType, Capability, CompilationUnitTypes, Copyability, IntrinsicTypeConstructor,
        TypeEnvironment, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitCallTarget, check_compilation_unit_types,
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

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BUILTINS.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        )
    });
    let copyable = names.declare_type("Copyable").expect("Copyable");
    let boxed = names.declare_type("Box").expect("Box");
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types.bind_builtin(symbol, builtin).expect("binding");
    }
    types
        .bind_capability(copyable, Capability::Copyable)
        .expect("Copyable binding");
    types
        .bind_intrinsic(boxed, IntrinsicTypeConstructor::Box)
        .expect("Box binding");
    (names, types)
}

// Each original oracle runs independently for both Unit entry modes. This
// intentionally retires Single observations, not the original source contracts.
fn checked_each(
    text: &str,
    mut inspect: impl FnMut(
        &SourceMap,
        &CompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
    ),
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("structural.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "structural ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(sources.source_text(source), Ok(text));
    let (environment, types) = environments();
    for input in [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "structural.ko", source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("source index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).expect("names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        // Basic validation preserves the original Phase 2 boundary; the bodyless
        // make() declaration below is not a final-admission witness.
        let typed = typed.validate().expect("valid types");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .expect("ownership");
        assert!(owned.is_compatible_with(&typed));
        for projection in typed.types().aggregate_projections() {
            assert_eq!(projection.expression().source_unit(), entry.id());
            assert_eq!(projection.field().source_unit(), entry.id());
        }
        inspect(&sources, names.names(), typed.types(), &owned);
    }
}

fn symbol_name(names: &CompilationUnitNames, symbol: UnitSymbolId) -> &str {
    let [entry] = names.index().source_units() else {
        panic!("one original source for field and call targets")
    };
    assert_eq!(symbol.source_unit(), entry.id());
    names.source_units()[symbol.source_unit().index()]
        .resolution()
        .symbols()[symbol.symbol().index()]
    .name()
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn conditional_copyability_drives_the_same_variable_state_machine() {
    let text = "class Resource {}\n\
                value class Pair<A, B>(val first: A, val second: B)\n\
                enum class Choice { Empty, Full(item: Resource) }\n\
                fun <T> genericMove(own item: T): Unit {\n\
                    val first = item\n\
                    val second = item\n\
                }\n\
                fun <T : Copyable> genericCopy(item: T): Unit {\n\
                    val first = item\n\
                    val second = item\n\
                }\n\
                fun matrix(\n\
                    copy: Pair<Int, Int>, own moved: Pair<Int, Resource>,\n\
                    own boxed: Box<Pair<Int, Int>>, own choice: Choice?\n\
                ): Unit {\n\
                    val copyFirst = copy\n\
                    val copySecond = copy\n\
                    val movedFirst = moved\n\
                    val movedSecond = moved\n\
                    val boxFirst = boxed\n\
                    val boxSecond = boxed\n\
                    val choiceFirst = choice\n\
                    val choiceSecond = choice\n\
                }";
    checked_each(text, |_, _, _, checked| {
        assert_eq!(codes(checked.diagnostics()), vec!["L0131"; 4]);
    });
}

#[test]
fn destructuring_copies_or_atomically_consumes_the_source_and_enables_bindings() {
    let text = "class Resource {}\n\
                value class Pair<A, B>(val first: A, val second: B)\n\
                fun take(own item: Resource): Unit {}\n\
                fun make(): Pair<Resource, Int>\n\
                fun copy(pair: Pair<Int, Int>): Unit {\n\
                    val (first, second) = pair\n\
                    val stillAvailable = pair\n\
                }\n\
                fun consume(own pair: Pair<Resource, Int>): Unit {\n\
                    val (resource, count) = pair\n\
                    val movedSource = pair\n\
                    val firstUse = take(resource)\n\
                    val secondUse = take(resource)\n\
                }\n\
                fun temporary(): Unit {\n\
                    val (resource, count) = make()\n\
                    val firstUse = take(resource)\n\
                    val secondUse = take(resource)\n\
                }";
    checked_each(text, |sources, _, _, checked| {
        assert_eq!(codes(checked.diagnostics()), vec!["L0131"; 3]);
        assert_eq!(
            checked
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).unwrap())
                .collect::<Vec<_>>(),
            ["pair", "resource", "resource"]
        );
    });
}

#[test]
fn field_and_automatic_component_facts_reject_only_non_copyable_owned_results() {
    let text = "class Resource {}\n\
                value class Bundle<T>(var payload: T, val count: Int)\n\
                value class Custom(val payload: Resource) {\n\
                    fun component1(): Int = 1\n\
                }\n\
                class Holder(val payload: Resource)\n\
                fun inspect(borrow item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun takeBundle(own item: Bundle<Resource>): Unit {}\n\
                fun takeCustom(own item: Custom): Unit {}\n\
                fun takeHolder(own item: Holder): Unit {}\n\
                fun project(own bundle: Bundle<Resource>, own custom: Custom, own holder: Holder): Unit {\n\
                    val borrowed = inspect(bundle.payload)\n\
                    val mutated = mutate(&bundle.payload)\n\
                    val copiedField = bundle.count\n\
                    val copiedComponent = bundle.component2()\n\
                    val invalidField = take(bundle.payload)\n\
                    val invalidComponent = bundle.component1()\n\
                    val explicitComponent = custom.component1()\n\
                    val whole = takeBundle(bundle)\n\
                    val moved = takeBundle(bundle)\n\
                    val customWhole = takeCustom(custom)\n\
                    val holderBorrowed = inspect(holder.payload)\n\
                    val invalidHolderField = take(holder.payload)\n\
                    val holderWhole = takeHolder(holder)\n\
                    val holderMoved = takeHolder(holder)\n\
                }";
    checked_each(text, |sources, names, typed, checked| {
        assert_eq!(
            codes(checked.diagnostics()),
            ["L0134", "L0132", "L0132", "L0131", "L0132", "L0131"]
        );
        assert_eq!(
            checked
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).unwrap())
                .collect::<Vec<_>>(),
            ["&", "payload", "component1", "bundle", "payload", "holder"]
        );
        for diagnostic in checked
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0132")
        {
            let label = diagnostic
                .details()
                .iter()
                .find_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => Some(label.span()),
                    DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
                })
                .expect("field label");
            assert_eq!(sources.slice(label).unwrap(), "payload");
        }

        let projections = typed.aggregate_projections();
        assert_eq!(
            projections
                .iter()
                .map(|projection| {
                    (
                        projection.kind(),
                        symbol_name(names, projection.field()),
                        typed.copyability(projection.ty()),
                    )
                })
                .collect::<Vec<_>>(),
            [
                (
                    UnitAggregateProjectionKind::Field,
                    "payload",
                    Copyability::MoveOnly,
                ),
                (
                    UnitAggregateProjectionKind::Field,
                    "payload",
                    Copyability::MoveOnly,
                ),
                (
                    UnitAggregateProjectionKind::Field,
                    "count",
                    Copyability::Copyable,
                ),
                (
                    UnitAggregateProjectionKind::StructuralComponent,
                    "count",
                    Copyability::Copyable,
                ),
                (
                    UnitAggregateProjectionKind::Field,
                    "payload",
                    Copyability::MoveOnly,
                ),
                (
                    UnitAggregateProjectionKind::StructuralComponent,
                    "payload",
                    Copyability::MoveOnly,
                ),
                (
                    UnitAggregateProjectionKind::Field,
                    "payload",
                    Copyability::MoveOnly,
                ),
                (
                    UnitAggregateProjectionKind::Field,
                    "payload",
                    Copyability::MoveOnly,
                ),
            ]
        );
        for projection in projections {
            let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
            else {
                panic!("structural fixture only contains explicit projection receivers");
            };
            assert_eq!(
                receiver.source_unit(),
                projection.expression().source_unit()
            );
            assert!(typed.expression_type(receiver).is_some());
            assert_eq!(
                typed.expression_type(projection.expression()),
                Some(projection.ty())
            );
        }
        assert!(typed.calls().iter().any(|call| matches!(
            call.target(),
            UnitCallTarget::StructuralComponent(field)
                if symbol_name(names, field) == "payload"
        )));
        assert!(typed.calls().iter().any(|call| matches!(
            call.target(),
            UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
        )));
        // The explicit member is a source Symbol, not the automatic component or
        // a top-level Declaration. Keep that source-call route independently visible.
        assert!(typed.calls().iter().any(|call| matches!(
            call.target(), UnitCallTarget::Symbol(symbol)
                if symbol_name(names, symbol) == "component1"
        )));
    });
}

#[test]
fn checked_in_phase3_structural_fixtures_execute_pass_and_fail_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase3");
    for (directory, should_pass) in [("structural-pass", true), ("structural-fail", false)] {
        let files = fs::read_dir(root.join(directory))
            .expect("structural fixture directory")
            .map(|entry| entry.expect("structural fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1, "zero or unexpected {directory} fixtures");
        for path in files {
            let text = fs::read_to_string(&path).expect("UTF-8 structural fixture");
            checked_each(&text, |_, _, _, checked| {
                if should_pass {
                    assert!(checked.diagnostics().is_empty(), "{path:?}");
                } else {
                    let expected = fs::read_to_string(path.with_extension("diag"))
                        .expect("diagnostic sidecar");
                    let actual = checked
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
            });
        }
    }
}
