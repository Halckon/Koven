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

#[test]
fn constant_expressions_reject_runtime_work_even_in_a_short_circuit_rhs() {
    for (prefix, expression, invalid) in [
        ("fun compute(): Int = 7\n", "compute()", "compute()"),
        ("val ordinary: Int = 7\n", "ordinary + 1", "ordinary"),
        (
            "fun probe(): Boolean = true\n",
            "false && probe()",
            "probe()",
        ),
        (
            "",
            "if (true) { 1 } else { 2 }",
            "if (true) { 1 } else { 2 }",
        ),
        ("", "\"answer ${1}\"", "\"answer ${1}\""),
        ("", "1.0 == 2.0", "1.0 == 2.0"),
    ] {
        let (sources, typed) = checked(&format!("{prefix}const val sample = {expression}"));
        assert_eq!(codes(typed.diagnostics()), ["L0156"], "{expression}");
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .unwrap(),
            invalid
        );
    }
}

#[test]
fn constant_dependency_cycles_include_short_circuit_edges() {
    for source in [
        "const val first: Int = first",
        "const val first: Int = second\nconst val second: Int = first",
        "const val first = second\nconst val second = first",
        "const val first: Boolean = false && second\nconst val second: Boolean = first",
    ] {
        let (sources, typed) = checked(source);
        assert_eq!(codes(typed.diagnostics()), ["L0157"], "{source}");
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .unwrap(),
            "first"
        );
        let (_, repeated) = checked(source);
        // 独立 analysis owner 不相等；比较完整可观察诊断，而非 owner token。
        assert_eq!(
            format!("{:?}", typed.diagnostics()),
            format!("{:?}", repeated.diagnostics())
        );
    }
}

#[test]
fn constant_expression_type_errors_and_invalid_dependencies_do_not_cascade() {
    for (source, expected) in [
        ("const val sample: Int = true", vec!["L0084"]),
        (
            "fun compute(): Int = 1\nconst val first = bad == 1.0\nconst val bad = compute()",
            vec!["L0156"],
        ),
        (
            "fun compute(): Int = 1\nconst val first: Int = second + compute()\nconst val second: Int = first",
            vec!["L0156"],
        ),
        (
            "const val first: Int = first\nconst val second: Int = second",
            vec!["L0157", "L0157"],
        ),
    ] {
        let (_, typed) = checked(source);
        assert_eq!(codes(typed.diagnostics()), expected, "{source}");
    }
    // Eligibility checks both operands but must not eagerly evaluate a short-circuited division.
    let (_, typed) = checked(
        "const val first: Boolean = false && (1 / 0 == 0)\nconst val second: Int = 1 + 2 * 3",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn constant_qualification_rechecks_forward_operand_types() {
    for source in [
        "const val sample = flag == flag\nconst val flag = true",
        "const val flag = true\nconst val sample = flag == flag",
        "const val sample = (other) == (other)\nconst val other = flag\nconst val flag = true",
    ] {
        let (_, typed) = checked(source);
        assert_eq!(codes(typed.diagnostics()), ["L0156"], "{source}");
    }
    for source in [
        "const val sample = flag == true\nconst val flag = 1",
        "const val flag = 1\nconst val sample = flag == true",
        "const val sample = flag == 1.0\nconst val flag = 1",
        "const val flag = 1\nconst val sample = flag == 1.0",
    ] {
        let (_, typed) = checked(source);
        assert_eq!(codes(typed.diagnostics()), ["L0085"], "{source}");
    }
}

#[test]
fn constant_qualification_rejects_companion_this_before_type_cascades() {
    for declaration in [
        "const val sample = this",
        "const val sample: Int = this",
        "const val sample = this.field",
        "const val sample = field",
        "const val sample: T = 1",
        "const val sample = 1 is T",
        "const val sample = T.member",
        "const val sample = this.member()",
        "const val sample = member()",
    ] {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "context.ko",
                format!(
                    "class Config<T>(val field: Int) {{ fun member(): Int = field\ncompanion object {{ {declaration} }} }}"
                ),
            )
            .unwrap();
        let parsed = parser_test_assertions::parse_file_twice(&sources, source, "constant context");
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &names).unwrap();
        assert_eq!(codes(names.diagnostics()), ["L0153"], "{declaration}");
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(
            typed.diagnostics().is_empty(),
            "{declaration}: {:?}",
            typed.diagnostics()
        );
        assert!(typed.constants().is_none());
    }
}

#[test]
fn constant_evaluation_reports_checked_arithmetic_failures() {
    for expression in [
        "2147483647 + 1",
        "(-2147483648) - 1",
        "100000 * 100000",
        "1 / 0",
        "1 % 0",
        "(-2147483648) / -1",
        "(-2147483648) % -1",
        "18446744073709551615uL + 1uL",
        "0u - 1u",
        "-(-2147483648)",
    ] {
        let (_, typed) = checked(&format!("const val sample = {expression}"));
        assert_eq!(codes(typed.diagnostics()), ["L0158"], "{expression}");
    }
}

#[test]
fn constant_evaluation_keeps_short_circuit_and_invalid_dependency_boundaries() {
    for source in [
        "const val sample = false && (1 / 0 == 0)",
        "const val sample = true || (2147483647 + 1 == 0)",
        "const val sample = (answer == 42) || (1 / 0 == 0)\nconst val answer = 6 * 7",
        "const val sample = (\"a\" + \"b\" == \"ab\") || (1 / 0 == 0)",
    ] {
        let (_, typed) = checked(source);
        assert!(
            typed.diagnostics().is_empty(),
            "{source}: {:?}",
            typed.diagnostics()
        );
    }
    let (_, typed) = checked("const val bad = 1 / 0\nconst val dependent = bad + 1");
    assert_eq!(codes(typed.diagnostics()), ["L0158"]);
}

#[test]
fn constant_evaluation_uses_associated_declared_integer_widths_and_operator_spans() {
    for (ty, max, one) in [
        ("Byte", "127", "1"),
        ("Short", "32767", "1"),
        ("Long", "9223372036854775807L", "1L"),
        ("UByte", "255u", "1u"),
        ("UShort", "65535u", "1u"),
    ] {
        let (sources, typed) = checked(&format!(
            "const val sample = Limits.MAX + Limits.ONE\nobject Limits {{ const val MAX: {ty} = {max}\nconst val ONE: {ty} = {one} }}"
        ));
        assert_eq!(codes(typed.diagnostics()), ["L0158"], "{ty}");
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .unwrap(),
            "+"
        );
    }
}

#[test]
fn forward_constants_are_typed_before_runtime_consumers() {
    let (sources, parsed, typed) =
        analyzed("fun answer(): Int = FIRST + 1\nconst val FIRST = SECOND\nconst val SECOND = 41");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let (expression, _) = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()).unwrap() == "FIRST + 1")
        .unwrap();
    assert_eq!(
        typed
            .types()
            .get(typed.expression_type(expression).unwrap()),
        Some(&TypeKind::Builtin(BuiltinType::Int))
    );
    let (_, typed) = checked("fun answer(): Int = FLAG + 1\nconst val FLAG = true");
    assert_eq!(codes(typed.diagnostics()), ["L0085"]);
}

#[test]
fn constant_facts_publish_exact_values_dependencies_and_runtime_reads() {
    use lang_frontend::type_checking::ConstValue;
    let source = "fun answer(): Int = Config.answer\nobject Config { const val answer = seed + 1\nconst val seed = 41 }\nconst val flag = true\nconst val byte: Byte = -128\nconst val short: Short = -32768\nconst val long: Long = -9223372036854775808L\nconst val ubyte: UByte = 255u\nconst val ushort: UShort = 65535u\nconst val uint = 4294967295u\nconst val ulong = 18446744073709551615uL\nconst val character = '中'\nconst val text = \"中\\n\" + \"文\"";
    let (sources, _, typed) = analyzed(source);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let facts = typed.constants().expect("complete constant capability");
    assert!(facts.matches(&typed));
    assert!(facts.matches(&typed.clone()));
    let (_, other) = checked(source);
    assert!(!facts.matches(&other));
    assert_eq!(facts.declarations().len(), 12);
    for declaration in facts.declarations() {
        let name = sources.slice(declaration.declaration_span()).unwrap();
        let expected = match name {
            "answer" => ConstValue::Integer {
                ty: BuiltinType::Int,
                value: 42,
            },
            "seed" => ConstValue::Integer {
                ty: BuiltinType::Int,
                value: 41,
            },
            "flag" => ConstValue::Boolean(true),
            "byte" => ConstValue::Integer {
                ty: BuiltinType::Byte,
                value: -128,
            },
            "short" => ConstValue::Integer {
                ty: BuiltinType::Short,
                value: -32768,
            },
            "long" => ConstValue::Integer {
                ty: BuiltinType::Long,
                value: -9223372036854775808,
            },
            "ubyte" => ConstValue::Integer {
                ty: BuiltinType::UByte,
                value: 255,
            },
            "ushort" => ConstValue::Integer {
                ty: BuiltinType::UShort,
                value: 65535,
            },
            "uint" => ConstValue::Integer {
                ty: BuiltinType::UInt,
                value: 4294967295,
            },
            "ulong" => ConstValue::Integer {
                ty: BuiltinType::ULong,
                value: 18446744073709551615,
            },
            "character" => ConstValue::Char('中'),
            "text" => ConstValue::String(std::sync::Arc::from("中\n文".as_bytes())),
            _ => panic!("unexpected constant {name}"),
        };
        assert_eq!(declaration.value(), &expected, "{name}");
        assert_eq!(
            typed.symbol_type(declaration.symbol()),
            Some(declaration.ty())
        );
        if name == "answer" {
            assert_eq!(declaration.dependencies().len(), 1);
        } else {
            assert!(declaration.dependencies().is_empty());
        }
    }
    assert_eq!(facts.uses().len(), 2);
    for usage in facts.uses() {
        let declaration = facts
            .declarations()
            .iter()
            .find(|declaration| declaration.symbol() == usage.target())
            .unwrap();
        assert_eq!(usage.value(), declaration.value());
        assert_eq!(typed.expression_type(usage.expression()), Some(usage.ty()));
        assert_eq!(usage.category(), ExpressionCategory::Temporary);
        assert_eq!(
            facts.use_at(usage.expression()).unwrap().target(),
            usage.target()
        );
    }
}

#[test]
fn invalid_constant_analysis_does_not_publish_partial_facts() {
    for source in [
        "const val good = 1\nconst val bad = 1 / 0",
        "const val good = 1\nconst val bad: Int = true",
        "const val good = 1\nconst val bad = bad",
        "const val good = 1\nval runtime: Int = true",
    ] {
        let (_, typed) = checked(source);
        assert!(typed.constants().is_none(), "{source}");
    }
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("unknown.ko", "const val good = 1\nval runtime = unknown")
        .unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "upstream error");
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    assert!(!names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.constants().is_none());
}
