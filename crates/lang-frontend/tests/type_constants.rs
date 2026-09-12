//! SPEC-0026：封闭常量类型与后续求值契约。

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::resolve_names,
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

fn checked(text: &str) -> (SourceMap, TypedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("constants.ko", text).expect("source");
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "constants");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    (sources, typed)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().map(|d| d.code().to_string()).collect()
}

#[test]
fn constant_types_are_closed_without_restricting_ordinary_variables() {
    // No nominal/nullable/global owner may be introduced through a const declaration.
    for (prefix, ty, value) in [
        ("", "Any", "1"),
        ("", "Any?", "1"),
        ("", "Double", "1.5"),
        ("", "Float", "1.5f"),
        ("", "Int?", "1"),
        ("class Node {}\n", "Node", "Node()"),
        ("value class Token(val sample: Int)\n", "Token", "Token(1)"),
        ("", "Array<Int>", "arrayOf(1)"),
        ("enum class Color { Red }\n", "Color", "Color.Red"),
        ("", "() -> Int", "{ 1 }"),
    ] {
        let (_, variable) = checked(&format!("{prefix}val sample: {ty} = {value}"));
        assert!(
            variable.diagnostics().is_empty(),
            "{ty}: {:?}",
            variable.diagnostics()
        );
        let (sources, constant) = checked(&format!("{prefix}const val sample: {ty} = {value}"));
        assert_eq!(codes(constant.diagnostics()), ["L0155"], "{ty}");
        assert_eq!(
            sources
                .slice(constant.diagnostics()[0].primary_span())
                .unwrap(),
            ty
        );
    }
}

#[test]
fn scalar_constant_types_accept_explicit_and_inferred_literals() {
    for (ty, value) in [
        ("Boolean", "true"),
        ("Byte", "1"),
        ("Short", "1"),
        ("Int", "1"),
        ("Long", "1L"),
        ("UByte", "1u"),
        ("UShort", "1u"),
        ("UInt", "1u"),
        ("ULong", "1uL"),
        ("Char", "'文'"),
        ("String", "\"text\""),
    ] {
        for annotation in [format!(": {ty}"), String::new()] {
            let (_, typed) = checked(&format!("const val sample{annotation} = {value}"));
            assert!(
                typed.diagnostics().is_empty(),
                "{ty}{annotation}: {:?}",
                typed.diagnostics()
            );
        }
    }
}

#[test]
fn invalid_inferred_constant_type_points_to_initializer_without_type_error_cascade() {
    let (sources, typed) = checked("const val sample = 1.5");
    assert_eq!(codes(typed.diagnostics()), ["L0155"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .unwrap(),
        "1.5"
    );
    for ty in ["Int", "Double"] {
        let (_, typed) = checked(&format!("const val sample: {ty} = true"));
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
    }
}

#[test]
fn constant_type_gate_applies_inside_object_and_companion_namespaces() {
    for source in [
        "object Config { const val sample: Any = 1 }",
        "class Config { companion object { const val sample: Any? = 1 } }",
    ] {
        let (_, typed) = checked(source);
        assert_eq!(codes(typed.diagnostics()), ["L0155"], "{source}");
    }
}
