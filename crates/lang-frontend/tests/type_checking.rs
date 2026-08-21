//! SPEC-0019 基础类型、局部推导和返回契约集成测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{NameEnvironment, NameResolution, resolve_names},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        BuiltinType, DeferredReason, TypeCheckingError, TypeEnvironment, TypeKind, TypedFile,
        check_types,
    },
};
use std::{collections::BTreeSet, fs, path::Path};

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
    let source = sources.add_source("types.ko", text).expect("source");
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
    let mut declarations = Vec::new();
    for builtin in BUILTINS {
        declarations.push((
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        ));
    }
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in declarations {
        types.bind_builtin(symbol, builtin).expect("binding");
    }
    (names, types)
}

fn checked(text: &str) -> (SourceMap, ParsedFile, NameResolution, TypedFile) {
    let (sources, parsed) = parse(text);
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

fn literal_type(
    sources: &SourceMap,
    parsed: &ParsedFile,
    typed: &TypedFile,
    text: &str,
) -> BuiltinType {
    let (id, _) = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()) == Ok(text))
        .expect("literal");
    let ty = typed.expression_type(id).expect("expression type");
    match typed.types().get(ty).expect("type") {
        TypeKind::Builtin(builtin) => *builtin,
        other => panic!("unexpected type {other:?}"),
    }
}

#[test]
fn numeric_defaults_suffixes_and_contextual_integer_types_are_stable() {
    let text = "fun values(): Unit {\n\
                val byte: Byte = 1\n\
                val default = 2\n\
                val wide = 2147483648\n\
                val unsigned = 3u\n\
                val unsignedWide = 4294967296u\n\
                val long = 4L\n\
                val unsignedLong = 5uL\n\
                val float = 6f\n\
                val double = 7.0\n\
                }";
    let (sources, parsed, _, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    for (literal, expected) in [
        ("1", BuiltinType::Byte),
        ("2", BuiltinType::Int),
        ("2147483648", BuiltinType::Long),
        ("3u", BuiltinType::UInt),
        ("4294967296u", BuiltinType::ULong),
        ("4L", BuiltinType::Long),
        ("5uL", BuiltinType::ULong),
        ("6f", BuiltinType::Float),
        ("7.0", BuiltinType::Double),
    ] {
        assert_eq!(literal_type(&sources, &parsed, &typed, literal), expected);
    }
}

#[test]
fn local_lambda_operator_and_type_ref_diagnostics_use_published_codes() {
    let (_, _, _, valid) = checked(
        "fun apply(): Unit {\n\
         val callback: (Int) -> Int = { x -> x + 1 }\n\
         val safe: Int? = null\n\
         val resolved = safe ?: 0\n\
         }",
    );
    assert!(valid.diagnostics().is_empty(), "{:?}", valid.diagnostics());

    let (_, _, _, invalid) = checked(
        "fun invalid(): Unit {\n\
         val missing = null\n\
         val mismatch: String = 1\n\
         val operand = false + 1\n\
         val arity: Int<String> = 1\n\
         }",
    );
    assert_eq!(
        codes(invalid.diagnostics()),
        ["L0083", "L0084", "L0085", "L0082"]
    );
}

#[test]
fn callable_flow_reports_shape_missing_return_and_branch_join() {
    let (_, _, _, typed) = checked(
        "fun missing(): Int {}\n\
         fun bare(): Int { return }\n\
         fun unit(): Unit = 1\n\
         fun branch(flag: Boolean): Int = if (flag) { 1 } else { false }\n\
         fun mixed(flag: Boolean): Unit { val result = if (flag) { 1 } else { false } }",
    );
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0088", "L0087", "L0084", "L0084", "L0089"]
    );
}

#[test]
fn bottom_non_null_and_numeric_boundaries_do_not_widen_silently() {
    let (_, _, _, typed) = checked(
        "val outside = return\n\
         fun boundaries(): Unit {\n\
             val min: Byte = -128\n\
             val max: Byte = 127\n\
             val unsigned: UByte = 255u\n\
             val optional: Int? = 1\n\
             val required = optional!!\n\
             val invalidAssert = 1!!\n\
             val negativeUnsigned = -1u\n\
             val overflow: Byte = 128\n\
             val noSignedToUnsigned: UInt = 1\n\
         }",
    );
    assert_eq!(
        codes(typed.diagnostics()),
        ["L0086", "L0085", "L0085", "L0090", "L0084"]
    );
}

#[test]
fn repeated_checks_are_deterministic_and_foreign_source_maps_fail() {
    let (sources, parsed) = parse("fun identity(input: Int): Int = input + 1");
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let first = check_types(&sources, &parsed, &resolution, &types).expect("first");
    let second = check_types(&sources, &parsed, &resolution, &types).expect("second");
    assert_eq!(first.types(), second.types());
    assert_eq!(first.diagnostics(), second.diagnostics());
    for (id, _) in parsed.ast().expressions().iter() {
        assert_eq!(first.expression_type(id), second.expression_type(id));
    }

    let foreign = SourceMap::new();
    assert!(matches!(
        check_types(&foreign, &parsed, &resolution, &types),
        Err(TypeCheckingError::Source(_))
    ));
}

#[test]
fn environment_identity_is_explicit_and_duplicate_or_wrong_bindings_fail_loud() {
    let (sources, parsed) = parse("val datum: Int = 1");
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    let (_, foreign_types) = environments();
    assert!(matches!(
        check_types(&sources, &parsed, &resolution, &foreign_types),
        Err(TypeCheckingError::MismatchedNameEnvironment)
    ));

    let mut duplicate = types.clone();
    let int = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "Int")
        .expect("Int")
        .id();
    assert!(matches!(
        duplicate.bind_builtin(int, BuiltinType::Int),
        Err(TypeCheckingError::InvalidExternalBinding)
    ));
}

#[test]
fn numeric_overflow_is_a_source_diagnostic_not_an_internal_failure() {
    let (_, _, _, typed) = checked("val huge = 340282366920938463463374607431768211456");
    assert_eq!(codes(typed.diagnostics()), ["L0090"]);
}

#[test]
fn later_phase_nodes_keep_distinct_deferred_reasons() {
    let text = "val forward = later\n\
                val later = 1\n\
                class Sample {\n\
                    fun inspect(flag: Boolean): Unit {\n\
                        val top: Any = this\n\
                        val nominal: Sample = this\n\
                        val qualified: Sample.Inner = this\n\
                        val member = this.field\n\
                        val called = this()\n\
                        val indexed = this[0]\n\
                        val casted = this as Int\n\
                        val propagated = this?\n\
                        val overloaded = inspect\n\
                        val selected = when { else -> 1 }\n\
                        val joined = if (flag) { this } else { this }\n\
                        val (left, right) = this\n\
                    }\n\
                }";
    let (sources, parsed) = parse(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(resolution.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty());

    let mut reasons = BTreeSet::new();
    for (id, _) in parsed.ast().expressions().iter() {
        if let Some(TypeKind::Deferred(reason)) = typed
            .expression_type(id)
            .and_then(|ty| typed.types().get(ty))
        {
            reasons.insert(*reason);
        }
    }
    for (id, _) in parsed.ast().type_refs().iter() {
        if let Some(TypeKind::Deferred(reason)) =
            typed.type_ref_type(id).and_then(|ty| typed.types().get(ty))
        {
            reasons.insert(*reason);
        }
    }
    for symbol in resolution.symbols() {
        if let Some(TypeKind::Deferred(reason)) = typed
            .symbol_type(symbol.id())
            .and_then(|ty| typed.types().get(ty))
        {
            reasons.insert(*reason);
        }
    }
    for expected in [
        DeferredReason::AnyValueRepresentation,
        DeferredReason::NominalOrTypeParameter,
        DeferredReason::QualifiedType,
        DeferredReason::ForwardValueType,
        DeferredReason::ThisType,
        DeferredReason::MemberAccess,
        DeferredReason::Call,
        DeferredReason::Index,
        DeferredReason::CastOrTypeTest,
        DeferredReason::ErrorPropagation,
        DeferredReason::OverloadSelection,
        DeferredReason::WhenTyping,
        DeferredReason::ControlJoin,
        DeferredReason::Destructuring,
    ] {
        assert!(
            reasons.contains(&expected),
            "missing {expected:?}: {reasons:?}"
        );
    }
}

#[test]
fn checked_in_phase2_type_fixtures_execute_real_pass_and_fail_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase2");
    for (directory, should_pass) in [("type-pass", true), ("type-fail", false)] {
        let files = fs::read_dir(root.join(directory))
            .expect("fixture directory")
            .map(|entry| entry.expect("fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1, "zero or unexpected {directory} fixtures");
        for path in files {
            let text = fs::read_to_string(&path).expect("UTF-8 fixture");
            let (_, _, _, typed) = checked(&text);
            if should_pass {
                assert!(typed.diagnostics().is_empty(), "{path:?}");
            } else {
                let expected =
                    fs::read_to_string(path.with_extension("diag")).expect("diagnostic sidecar");
                let actual = typed
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
