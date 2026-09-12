//! SPEC-0209: consume validated constant/materialization facts, never initializer syntax.
use super::*;
use crate::ssa::model::{ScalarConstant, SsaTypeKind};

fn lower_constants(text: &str) -> (Analysis, crate::ssa::model::Program) {
    let analysis = analyze(text);
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("constant facts must lower");
    (analysis, program)
}

#[test]
fn associated_constants_lower_across_all_declaration_namespaces() {
    for declaration in [
        "object Labels { const val COUNT = BASE + 1 }",
        "class Labels { companion object { const val COUNT = BASE + 1 } }",
        "value class Labels(val number: Int) { companion object { const val COUNT = BASE + 1 } }",
        "interface Labels { companion object { const val COUNT = BASE + 1 } }",
        "enum class Labels { One; companion object { const val COUNT = BASE + 1 } }",
    ] {
        let (_, program) = lower_constants(&format!(
            "const val BASE = 40\n{declaration}\nfun answer(): Int = Labels.COUNT"
        ));
        let operations = program.modules[0].functions[0]
            .instructions
            .iter()
            .map(|instruction| &instruction.operation)
            .collect::<Vec<_>>();
        assert_eq!(
            operations,
            [&Operation::Constant(ScalarConstant::Integer(41))]
        );
        let llvm = render_verified_program(&program).unwrap();
        assert!(llvm.contains("ret i32 41"), "{llvm}");
        assert!(!llvm.contains("global"), "{llvm}");
    }
}

#[test]
fn exact_scalar_and_char_values_keep_their_ssa_types() {
    for (ty, literal, expected_return) in [
        ("Boolean", "true", "ret i1 true"),
        ("Byte", "127", "ret i8 127"),
        ("Short", "32767", "ret i16 32767"),
        ("Int", "2147483647", "ret i32 2147483647"),
        (
            "Long",
            "9223372036854775807L",
            "ret i64 9223372036854775807",
        ),
        ("UByte", "255u", "ret i8 -1"),
        ("UShort", "65535u", "ret i16 -1"),
        ("UInt", "4294967295u", "ret i32 -1"),
        ("ULong", "18446744073709551615uL", "ret i64 -1"),
        ("Char", "'文'", "ret i32 25991"),
    ] {
        let (_, program) = lower_constants(&format!(
            "const val VALUE: {ty} = {literal}\nfun answer(): {ty} = VALUE"
        ));
        let llvm = render_verified_program(&program).unwrap();
        assert!(llvm.contains(expected_return), "{ty}: {llvm}");
        if ty == "Char" {
            assert!(program.modules[0].types.contains(&SsaTypeKind::Char));
            assert!(llvm.contains("ret i32 25991"), "{llvm}");
        }
    }
}

#[test]
fn string_constant_reads_materialize_independent_literal_owners() {
    let (analysis, program) = lower_constants(
        r#"
const val TEXT = "中" + "文"
fun view(text: String): Unit {}
fun use(): Boolean {
    val first = view((TEXT))
    return TEXT + TEXT == TEXT
}
"#,
    );
    let literals = program.modules[0]
        .functions
        .iter()
        .flat_map(|function| &function.instructions)
        .filter_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes, .. } => Some(bytes.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(literals, ["中文".as_bytes(); 4]);
    let repeated = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap();
    assert_eq!(render_program(&program), render_program(&repeated));
    assert_eq!(
        render_verified_program(&program).unwrap(),
        render_verified_program(&repeated).unwrap()
    );
}

#[test]
fn unused_constant_namespaces_emit_only_the_entry() {
    let (_, program) = lower_constants(
        "const val UNUSED = 40 + 2\nobject Config { const val TEXT = \"unused\" }\nfun entry(): Unit {}",
    );
    let llvm = render_verified_program(&program).unwrap();
    assert_eq!(program.modules[0].functions.len(), 1);
    assert!(program.modules[0].functions[0].instructions.is_empty());
    for absent in ["UNUSED", "Config", "unused", "global", "init_guard"] {
        assert!(!llvm.contains(absent), "{llvm}");
    }
}
