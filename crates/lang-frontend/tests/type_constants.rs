//! SPEC-0026：封闭常量类型与后续求值契约。

use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::resolve_names,
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        BuiltinType, ExpressionCategory, TypeKind, TypedFile, check_types, standard_environments,
    },
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

fn checked(text: &str) -> (SourceMap, TypedFile) {
    let (sources, _, typed) = analyzed(text);
    (sources, typed)
}

fn analyzed(text: &str) -> (SourceMap, ParsedFile, TypedFile) {
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
    (sources, parsed, typed)
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

#[test]
fn associated_constant_selection_uses_declaration_namespace_and_concrete_type() {
    for declaration in [
        "class Config { companion object { const val LIMIT: Int = 7 } }",
        "value class Config(val item: Int) { companion object { const val LIMIT: Int = 7 } }",
        "interface Config { companion object { const val LIMIT: Int = 7 } }",
        "enum class Config { One; companion object { const val LIMIT: Int = 7 } }",
        "object Config { const val LIMIT: Int = 7 }",
    ] {
        // Concrete Int facts must be available even if the use precedes the declaration.
        for source in [
            format!("{declaration}\nval selected = Config.LIMIT"),
            format!("val selected = Config.LIMIT\n{declaration}"),
        ] {
            let (sources, parsed, typed) = analyzed(&source);
            assert!(
                typed.diagnostics().is_empty(),
                "{source}: {:?}",
                typed.diagnostics()
            );
            let (expression, _) = parsed
                .ast()
                .expressions()
                .iter()
                .find(|(_, node)| sources.slice(node.span()).unwrap() == "Config.LIMIT")
                .expect("member expression");
            assert_eq!(
                typed.expression_category(expression),
                Some(ExpressionCategory::Temporary)
            );
            assert_eq!(
                typed
                    .types()
                    .get(typed.expression_type(expression).unwrap()),
                Some(&TypeKind::Builtin(BuiltinType::Int)),
                "{source}"
            );
        }
    }
}

#[test]
fn associated_constant_visibility_and_missing_member_are_precise() {
    for (declaration, target, code) in [
        (
            "class Config { companion object { private const val LIMIT: Int = 7 } }",
            "Config.LIMIT",
            "L0154",
        ),
        (
            "object Config { private const val LIMIT: Int = 7 }",
            "Config.LIMIT",
            "L0154",
        ),
        ("class Config {}", "Config.MISSING", "L0080"),
        (
            "interface Protocol { companion object { const val LIMIT: Int = 7 } }\nclass Config : Protocol {}",
            "Config.LIMIT",
            "L0080",
        ),
    ] {
        let (sources, typed) = checked(&format!("{declaration}\nval selected = {target}"));
        assert_eq!(codes(typed.diagnostics()), [code], "{target}");
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .unwrap(),
            target.split('.').next_back().unwrap()
        );
    }
    let (_, typed) = checked(
        "class Config { companion object { private const val LIMIT: Int = 7\nconst val COPY: Int = Config.LIMIT } }",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn associated_constant_selection_does_not_override_a_value_receiver() {
    let source = "class Config { companion object { const val LIMIT: Int = 7 } }\nclass Holder(val LIMIT: Boolean)\nfun inspect(Config: Holder): Boolean = Config.LIMIT";
    let (sources, parsed, typed) = analyzed(source);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let (expression, _) = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()).unwrap() == "Config.LIMIT")
        .unwrap();
    assert_eq!(
        typed
            .types()
            .get(typed.expression_type(expression).unwrap()),
        Some(&TypeKind::Builtin(BuiltinType::Boolean))
    );
    assert_eq!(
        typed.expression_category(expression),
        Some(ExpressionCategory::Place)
    );
}

#[test]
fn associated_constant_selection_does_not_create_an_inout_place() {
    let (_, typed) = checked(
        "object Config { const val LIMIT: Int = 7 }\nfun mutate(inout item: Int): Unit {}\nfun inspect(): Unit { mutate(&Config.LIMIT) }",
    );
    assert_eq!(codes(typed.diagnostics()), ["L0122"]);
}

#[test]
fn associated_constant_inout_is_rejected_after_overload_type_filtering() {
    let (_, typed) = checked(
        "object Config { const val LIMIT: Int = 7 }\nfun mutate(inout item: Int): Unit {}\nfun mutate(inout item: Boolean): Unit {}\nfun inspect(): Unit { mutate(&Config.LIMIT) }",
    );
    assert_eq!(codes(typed.diagnostics()), ["L0122"]);
}
