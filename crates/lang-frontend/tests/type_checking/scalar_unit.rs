//! 原始数值与局部/返回诊断消费者使用双 Unit raw type 入口。
use super::{codes, unit_entries::checked_entries};
use lang_frontend::{
    name_resolution::SourceUnitId,
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{BuiltinType, CompilationUnitTypes, UnitExpressionId, UnitTypeKind},
};

fn literal_type(
    sources: &SourceMap,
    parsed: &ParsedFile,
    source: SourceUnitId,
    typed: &CompilationUnitTypes,
    text: &str,
) -> BuiltinType {
    let (id, _) = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()) == Ok(text))
        .expect("literal");
    let ty = typed
        .expression_type(UnitExpressionId::new(source, id))
        .expect("expression type");
    match typed.types().get(ty).expect("type") {
        UnitTypeKind::Builtin(builtin) => *builtin,
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
    let (sources, parsed, entries) = checked_entries(text);
    for (source, typed) in entries {
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
            assert_eq!(
                literal_type(&sources, &parsed, source, &typed, literal),
                expected
            );
        }
    }
}

#[test]
fn local_lambda_operator_and_type_ref_diagnostics_use_published_codes() {
    let (_, _, valid) = checked_entries(
        "fun apply(): Unit {\n\
         val callback: (Int) -> Int = { x -> x + 1 }\n\
         val safe: Int? = null\n\
         val resolved = safe ?: 0\n\
         }",
    );
    for (_, valid) in valid {
        assert!(valid.diagnostics().is_empty(), "{:?}", valid.diagnostics());
    }

    let (_, _, invalid) = checked_entries(
        "fun invalid(): Unit {\n\
         val missing = null\n\
         val mismatch: String = 1\n\
         val operand = false + 1\n\
         val arity: Int<String> = 1\n\
         }",
    );
    for (_, invalid) in invalid {
        assert_eq!(
            codes(invalid.diagnostics()),
            ["L0083", "L0084", "L0085", "L0082"]
        );
    }
}

#[test]
fn callable_flow_reports_shape_missing_return_and_branch_join() {
    let (_, _, entries) = checked_entries(
        "fun missing(): Int {}\n\
         fun bare(): Int { return }\n\
         fun unit(): Unit = 1\n\
         fun branch(flag: Boolean): Int = if (flag) { 1 } else { false }\n\
         fun mixed(flag: Boolean): Unit { val result = if (flag) { 1 } else { false } }",
    );
    for (_, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0088", "L0087", "L0084", "L0084", "L0089"]
        );
    }
}

#[test]
fn unbraced_if_branches_receive_the_outer_expected_type() {
    let (sources, _, entries) =
        checked_entries("fun contextual(flag: Boolean): Byte = if (flag) 127 else 128");

    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0090"]);
        assert_eq!(
            sources.slice(typed.diagnostics()[0].primary_span()),
            Ok("128")
        );
    }
}
