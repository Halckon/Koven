//! Constant-owner materialization through the ordinary interpolation route.
use super::*;

#[test]
fn constant_native_lowering_interpolation_uses_string_conversions_and_exits() {
    for literal in [
        r#""${p.TEXT}""#,
        r#""${"${p.TEXT}"}""#,
        r#""${if (flag) { return p.TEXT } else { p.TEXT }}""#,
    ] {
        let consumer = format!("fun entry(own flag: Boolean): String = {literal}");
        let program = lower_short_circuit_fixture(&consumer);
        assert!(
            program.modules[0].functions.iter().any(|function| {
                function.instructions.iter().any(|instruction| {
                    matches!(instruction.operation, Operation::StringAppend { .. })
                })
            }),
            "{literal}"
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
    let program = lower_short_circuit_fixture(r#"fun entry(): String = "${error(p.TEXT)}""#);
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn constant_native_lowering_interpolation_converts_cross_file_boolean() {
    let program = lower_short_circuit_fixture(r#"fun entry(): String = "${p.TRUE}|${p.FALSE}""#);
    for spelling in [b"true".as_slice(), b"false"] {
        assert!(program.modules[0].functions.iter().any(|function| {
            function.instructions.iter().any(|instruction| {
                matches!(&instruction.operation, Operation::StringLiteral { bytes, .. } if bytes == spelling)
            })
        }));
    }
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn constant_native_lowering_interpolation_rejects_unsupported_float_conversion() {
    let literal = r#""prefix ${1.0} suffix""#;
    let consumer = format!("fun entry(own flag: Boolean): String = {literal}");
    let source = format!("package q\n{consumer}");
    for _ in 0..2 {
        let error = try_lower_constant_fixture(&consumer).err().unwrap();
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let span = error
            .span
            .expect("rejection identifies the whole interpolation");
        assert_eq!(&source[span.start()..span.end()], literal);
    }
}
