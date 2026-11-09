//! SPEC-0028 条件复制、消费式解构与结构分量移动测试。

use std::{fs, path::Path};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameEnvironment, NameResolution, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        AggregateProjectionKind, AggregateProjectionReceiver, BuiltinType, CallableTarget,
        Capability, Copyability, IntrinsicTypeConstructor, TypeEnvironment, TypedFile, check_types,
    },
};

use lang_frontend::{
    name_resolution::{
        CompilationUnitNames, SourceUnitInput, SourceUnitKey, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    type_checking::{CompilationUnitTypes, check_compilation_unit_types},
};

#[path = "ownership_structural/component_partial_move.rs"]
mod component_partial_move;

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

#[test]
fn value_class_does_not_supply_an_undeclared_component_method() {
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};

    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "component.ko",
            "class Resource {}\n\
        value class Pair<T>(val first: T)\n\
        fun read(pair: Pair<Int>): Int = pair.component1()\n\
        fun movePart(pair: Pair<Resource>): Resource = pair.component1()",
        )
        .unwrap();
    let parsed = parse_file_twice(&sources, source, "undeclared component method");
    let (environment, types) = environments();
    for (rooted, input) in [
        (false, SourceUnitInput::memory_document(source, &parsed)),
        (
            true,
            SourceUnitInput::new("root", "component.ko", source, &parsed),
        ),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original component source");
        };
        assert_eq!(entry.source_id(), source);
        match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "component.ko");
            }
            _ => panic!("requested component entry identity"),
        }
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        let Err(FinalTypeAdmissionError::Diagnostics(unit)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
        else {
            panic!("unit analysis must reject undeclared methods with source diagnostics");
        };
        assert!(typed.calls().is_empty());
        assert!(typed.aggregate_projections().is_empty());
        assert_eq!(codes(&unit), ["L0165", "L0165"]);
        assert!(unit.iter().all(|diagnostic| {
            diagnostic.primary_span().source_id() == source
                && sources.slice(diagnostic.primary_span()) == Ok("pair.component1")
                && diagnostic.details().is_empty()
        }));
    }
}

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

fn checked(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    NameResolution,
    TypedFile,
    OwnershipCheckedFile,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("structural.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "structural ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    (sources, parsed, names, typed, owned)
}

// Retain the Single projection and fixture oracles alongside the Unit ownership checks.
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
                    val (second, first) = pair\n\
                    val movedSource = pair\n\
                    val firstUse = take(first)\n\
                    val secondUse = take(first)\n\
                }\n\
                fun temporary(): Unit {\n\
                    val (second, first) = make()\n\
                    val firstUse = take(first)\n\
                    val secondUse = take(first)\n\
                }";
    checked_each(text, |sources, _, _, checked| {
        assert_eq!(codes(checked.diagnostics()), vec!["L0131"; 3]);
        assert_eq!(
            checked
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).unwrap())
                .collect::<Vec<_>>(),
            ["pair", "first", "first"]
        );
    });
}

#[test]
fn field_projection_and_explicit_methods_preserve_ownership_contracts() {
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
                    val invalidField = take(bundle.payload)\n\
                    val explicitComponent = custom.component1()\n\
                    val whole = takeBundle(bundle)\n\
                    val moved = takeBundle(bundle)\n\
                    val customWhole = takeCustom(custom)\n\
                    val holderBorrowed = inspect(holder.payload)\n\
                    val invalidHolderField = take(holder.payload)\n\
                    val holderWhole = takeHolder(holder)\n\
                    val holderMoved = takeHolder(holder)\n\
                }";
    let (sources, _, names, typed, checked) = checked(text);
    assert_eq!(
        codes(checked.diagnostics()),
        ["L0134", "L0132", "L0131", "L0132", "L0131"]
    );
    assert_eq!(
        checked
            .diagnostics()
            .iter()
            .map(|diagnostic| sources.slice(diagnostic.primary_span()).unwrap())
            .collect::<Vec<_>>(),
        ["&", "payload", "bundle", "payload", "holder"]
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
                    names.symbols()[projection.field().index()].name(),
                    typed.copyability(projection.ty()).unwrap(),
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                AggregateProjectionKind::Field,
                "payload",
                Copyability::MoveOnly,
            ),
            (
                AggregateProjectionKind::Field,
                "payload",
                Copyability::MoveOnly,
            ),
            (
                AggregateProjectionKind::Field,
                "count",
                Copyability::Copyable,
            ),
            (
                AggregateProjectionKind::Field,
                "payload",
                Copyability::MoveOnly,
            ),
            (
                AggregateProjectionKind::Field,
                "payload",
                Copyability::MoveOnly,
            ),
            (
                AggregateProjectionKind::Field,
                "payload",
                Copyability::MoveOnly,
            ),
        ]
    );
    for projection in projections {
        let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            panic!("structural fixture only contains explicit projection receivers");
        };
        assert!(typed.expression_type(receiver).is_some());
        assert_eq!(
            typed.expression_type(projection.expression()),
            Some(projection.ty())
        );
    }
    assert!(
        typed
            .calls()
            .iter()
            .any(|call| matches!(call.target(), CallableTarget::Source(_)))
    );
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
            let (_, _, _, _, checked) = checked(&text);
            if should_pass {
                assert!(checked.diagnostics().is_empty(), "{path:?}");
            } else {
                let expected =
                    fs::read_to_string(path.with_extension("diag")).expect("diagnostic sidecar");
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
        }
    }
}

#[test]
fn legacy_destructuring_keeps_named_bindings_aligned_with_their_field_types() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "legacy-named-split.ko",
            "value class Pair(val first: Int, val second: String)\n\
         fun reversed(own pair: Pair): Int { val (second, first) = pair; return first }\n\
         fun unknown(own pair: Pair): Unit { val (first, arbitrary) = pair }",
        )
        .unwrap();
    let parsed = parse_file_twice(&sources, source, "legacy named split");
    let (names_env, types_env) = environments();
    let names = resolve_names(&sources, &parsed, &names_env).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types_env).unwrap();
    assert_eq!(codes(typed.diagnostics()), ["L0080"]);
    assert_eq!(
        sources.slice(typed.diagnostics()[0].primary_span()),
        Ok("arbitrary")
    );
    let [descriptor] = typed.destructurings() else {
        panic!("only the complete named split publishes facts");
    };
    let bindings = descriptor
        .components()
        .iter()
        .map(|component| {
            let symbol = &names.symbols()[component.symbol().index()];
            sources.slice(symbol.span()).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(bindings, ["first", "second"]);
}

#[test]
fn legacy_private_fields_require_the_lexical_classifier() {
    let mut sources = SourceMap::new();
    let text = "value class Vault(private val hidden: Int, val open: Int) {\n\
                    fun unpack(other: Vault): Int { val (hidden, open) = other; return hidden }\n\
                    fun field(other: Vault): Int = other.hidden\n\
                }\n\
                fun unpack(v: Vault): Int { val (open, hidden) = v; return open }\n\
                fun field(v: Vault): Int = v.hidden\n\
                fun safe(v: Vault?): Int? = v?.hidden";
    let source = sources.add_source("legacy-private-field.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, source, "legacy private field");
    let (names_env, types_env) = environments();
    let names = resolve_names(&sources, &parsed, &names_env).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types_env).unwrap();
    assert_eq!(codes(typed.diagnostics()), ["L0080"; 3]);
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|d| sources.slice(d.primary_span()).unwrap())
            .collect::<Vec<_>>(),
        ["hidden", "hidden", "hidden"],
    );
    assert_eq!(typed.destructurings().len(), 1);
    assert_eq!(typed.aggregate_projections().len(), 1);
    assert!(typed.calls().is_empty());
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_file_types};
    let admission = complete_file_types(&sources, &parsed, &names, &types_env, &typed);
    let Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics)) = admission else {
        panic!("private patterns must reject with source diagnostics: {admission:?}");
    };
    assert_eq!(final_diagnostics, typed.diagnostics());
}

#[test]
fn legacy_borrowed_destructuring_does_not_read_discarded_private_fields() {
    use lang_frontend::type_checking::standard_environments;

    for (binding, result, expected) in [
        ("secret", "secret", &["L0160"][..]),
        ("_", "visible", &[][..]),
    ] {
        let text = format!(
            "value class Vault(private val hidden: Int, val open: Int)\n\
             fun inspect(items: List<Vault>): Int {{\n\
                 for (({binding}, visible) in items) {{ return {result} }}\n\
                 return 0\n\
             }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("legacy-private-for.ko", text).unwrap();
        let parsed = parse_file_twice(&sources, source, "legacy private for");
        let (names_env, types_env) = standard_environments();
        let names = resolve_names(&sources, &parsed, &names_env).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &parsed, &names, &types_env).unwrap();
        assert_eq!(codes(typed.diagnostics()), expected);
        assert_eq!(
            typed.sequential_iterations().len(),
            usize::from(expected.is_empty())
        );
    }
}

#[test]
fn legacy_private_destructuring_recovers_after_a_missing_pattern_closer() {
    use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_file_types};

    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "legacy-private-missing-closer.ko",
            "value class Vault(private val hidden: Int, val open: Int)\n\
         fun reject(v: Vault): Int { val (open, hidden = v; return open }",
        )
        .unwrap();
    let parsed = parse_file_twice(&sources, source, "legacy private missing closer");
    assert_eq!(codes(parsed.diagnostics()), ["L0010"]);
    let (names_env, types_env) = environments();
    let names = resolve_names(&sources, &parsed, &names_env).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types_env).unwrap();
    assert_eq!(codes(typed.diagnostics()), ["L0080"]);
    assert!(typed.destructurings().is_empty());
    let result = complete_file_types(&sources, &parsed, &names, &types_env, &typed);
    let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = result else {
        panic!("expected parser and private-field diagnostics: {result:?}");
    };
    assert_eq!(codes(&diagnostics), ["L0080", "L0010"]);
}
