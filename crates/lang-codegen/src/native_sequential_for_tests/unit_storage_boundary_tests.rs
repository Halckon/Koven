//! Unit storage does not weaken the native analysis-identity/publication boundary.

use super::super::{
    NativeObjectErrorKind, SymbolKind, TestDirectory, analyze, emit_native_object, symbol,
};

#[test]
fn unit_storage_mixed_analysis_preserves_existing_object() {
    let first = analyze(
        "unit-capability.ko",
        r#"
        fun unit(): Unit { println("first") }
        fun main(): Unit { for (item in arrayOf<Unit>(unit())) { val copy: Unit = item } }
        "#,
    );
    let second = analyze(
        "unit-capability.ko",
        r#"
        fun unit(): Unit { println("other") }
        fun main(): Unit { for (item in arrayOf<Unit>(unit())) { val copy: Unit = item } }
        "#,
    );
    for analysis in [&first, &second] {
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
    }
    let directory = TestDirectory::create();
    let output = directory.join("preserved.o");
    std::fs::write(&output, b"previous object bytes").unwrap();
    for (typed, owned) in [
        (&first.typed, &second.owned),
        (&second.typed, &second.owned),
    ] {
        let error = emit_native_object(
            &first.sources,
            &first.parsed,
            &first.names,
            typed,
            owned,
            symbol(&first, "main", SymbolKind::Function),
            &output,
        )
        .expect_err("foreign Unit facts must fail before object publication");
        assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
        assert_eq!(std::fs::read(&output).unwrap(), b"previous object bytes");
    }
}
