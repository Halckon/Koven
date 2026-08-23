//! SPEC-0027 所有权阶段边界与变量状态测试。

use std::{fs, path::Path};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{NameEnvironment, resolve_names},
    ownership_checking::{OwnershipCheckedFile, OwnershipCheckingError, check_ownership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{BuiltinType, TypeEnvironment, check_types},
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
    let declarations = BUILTINS.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin"),
            builtin,
        )
    });
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in declarations {
        types.bind_builtin(symbol, builtin).expect("binding");
    }
    (names, types)
}

fn checked(text: &str) -> (SourceMap, ParsedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("ownership.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "ownership source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).ok()
            ))
            .collect::<Vec<_>>()
    );
    let (environment, types) = environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    (sources, parsed, owned)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn ownership_stage_preserves_source_identity_and_rejects_mismatched_inputs() {
    let mut sources = SourceMap::new();
    let first_source = sources
        .add_source("first.ko", "val first = 1")
        .expect("first");
    let second_source = sources
        .add_source("second.ko", "val second = 2")
        .expect("second");
    let first = parse_file_twice(&sources, first_source, "first ownership source");
    let second = parse_file_twice(&sources, second_source, "second ownership source");
    let (environment, types) = environments();
    let first_names = resolve_names(&sources, &first, &environment).expect("first names");
    let second_names = resolve_names(&sources, &second, &environment).expect("second names");
    let first_typed = check_types(&sources, &first, &first_names, &types).expect("first types");
    let second_typed = check_types(&sources, &second, &second_names, &types).expect("second types");

    let checked = check_ownership(&sources, &first, &first_names, &first_typed).expect("ownership");
    assert_eq!(checked.source_id(), first_source);
    assert!(checked.diagnostics().is_empty());
    assert!(matches!(
        check_ownership(&sources, &first, &second_names, &first_typed),
        Err(OwnershipCheckingError::MismatchedNameSource)
    ));
    assert!(matches!(
        check_ownership(&sources, &first, &first_names, &second_typed),
        Err(OwnershipCheckingError::MismatchedTypedSource)
    ));
    assert!(matches!(
        check_ownership(&SourceMap::new(), &first, &first_names, &first_typed),
        Err(OwnershipCheckingError::Source(_))
    ));
}

#[test]
fn value_delivery_moves_only_once_while_copy_and_borrow_preserve_sources() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun create(): Resource\n\
                fun inspect(item: Resource): Unit {}\n\
                fun mutate(inout item: Resource): Unit {}\n\
                fun takeNumber(own item: Int): Unit {}\n\
                fun local(own input: Resource): Unit {\n\
                    val moved = input\n\
                    val after = take(input)\n\
                }\n\
                fun calls(own input: Resource): Unit {\n\
                    val first = take(input)\n\
                    val second = take(input)\n\
                    val third = take(input)\n\
                }\n\
                fun copy(number: Int): Unit {\n\
                    val first = number\n\
                    val second = number\n\
                    val firstCall = takeNumber(number)\n\
                    val secondCall = takeNumber(number)\n\
                }\n\
                fun temporary(): Unit {\n\
                    val first = take(create())\n\
                    val second = take(create())\n\
                }\n\
                fun borrows(own input: Resource): Unit {\n\
                    var local = input\n\
                    val first = inspect(local)\n\
                    val second = inspect(borrow local)\n\
                    val third = mutate(&local)\n\
                    val fourth = take(local)\n\
                    val fifth = take(local)\n\
                }\n\
                fun reset(own input: Resource, own replacement: Resource): Unit {\n\
                    var local = input\n\
                    val first = take(local)\n\
                    { local = replacement }\n\
                    val second = take(local)\n\
                    val third = take(local)\n\
                    { local = local }\n\
                    val fourth = take(local)\n\
                }";
    let (sources, _, checked) = checked(text);
    assert_eq!(codes(checked.diagnostics()), vec!["L0131"; 7]);
    let expected_primaries = [
        "input", "input", "input", "local", "local", "local", "local",
    ];
    for (diagnostic, expected) in checked.diagnostics().iter().zip(expected_primaries) {
        assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), expected);
        let label = diagnostic
            .details()
            .iter()
            .find_map(|detail| match detail {
                DiagnosticDetail::Label(label) => Some(label.span()),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .expect("move label");
        assert_eq!(sources.slice(label).unwrap(), expected);
    }
}

#[test]
fn error_nodes_do_not_create_ownership_cascades() {
    let text = "class Resource {}\n\
                fun broken(input: Resource): Unit {\n\
                    val missing =\n\
                }";
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("ownership-error.ko", text)
        .expect("source");
    let parsed = parse_file_twice(&sources, source, "ownership error source");
    assert!(!parsed.diagnostics().is_empty());
    let (environment, types) = environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let checked = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(checked.diagnostics().is_empty());
}

#[test]
fn control_flow_joins_possible_moves_but_ignores_returning_paths() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun branch(flag: Boolean, own input: Resource): Unit {\n\
                    if (flag) { take(input) }\n\
                    take(input)\n\
                }\n\
                fun choose(number: Int, own input: Resource): Unit {\n\
                    when (number) {\n\
                        0 -> take(input)\n\
                        else -> {}\n\
                    }\n\
                    take(input)\n\
                }\n\
                fun looping(flag: Boolean, own input: Resource): Unit {\n\
                    while (flag) {\n\
                        take(input)\n\
                        break\n\
                    }\n\
                    take(input)\n\
                }\n\
                fun terminating(flag: Boolean, own input: Resource): Resource {\n\
                    if (flag) { return input }\n\
                    return input\n\
                }";
    let (sources, _, checked) = checked(text);
    assert_eq!(codes(checked.diagnostics()), vec!["L0131"; 3]);
    for diagnostic in checked.diagnostics() {
        assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "input");
    }
}

#[test]
fn shadowed_symbols_keep_independent_move_origins() {
    let text = "class Resource {}\n\
                fun take(own item: Resource): Unit {}\n\
                fun shadow(own input: Resource): Unit {\n\
                    {\n\
                        val input = input\n\
                        val first = take(input)\n\
                        val second = take(input)\n\
                    }\n\
                    val outer = take(input)\n\
                }";
    let (sources, _, checked) = checked(text);
    assert_eq!(codes(checked.diagnostics()), ["L0131", "L0131"]);

    let outer_origin = text.find("val input = input").unwrap() + "val input = ".len();
    let inner_origin = text.find("val first = take(input)").unwrap() + "val first = take(".len();
    let inner_use = text.find("val second = take(input)").unwrap() + "val second = take(".len();
    let outer_use = text.find("val outer = take(input)").unwrap() + "val outer = take(".len();
    for (diagnostic, primary, origin) in [
        (&checked.diagnostics()[0], inner_use, inner_origin),
        (&checked.diagnostics()[1], outer_use, outer_origin),
    ] {
        assert_eq!(
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ),
            (primary, primary + "input".len())
        );
        let label = diagnostic
            .details()
            .iter()
            .find_map(|detail| match detail {
                DiagnosticDetail::Label(label) => Some(label.span()),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .expect("move label");
        assert_eq!(
            (label.start(), label.end()),
            (origin, origin + "input".len())
        );
        assert_eq!(sources.slice(label).unwrap(), "input");
    }
}

#[test]
fn checked_in_phase3_ownership_fixtures_execute_pass_and_fail_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase3");
    for (directory, should_pass) in [("ownership-pass", true), ("ownership-fail", false)] {
        let files = fs::read_dir(root.join(directory))
            .expect("ownership fixture directory")
            .map(|entry| entry.expect("ownership fixture entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1, "zero or unexpected {directory} fixtures");
        for path in files {
            let text = fs::read_to_string(&path).expect("UTF-8 ownership fixture");
            let (_, _, checked) = checked(&text);
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
