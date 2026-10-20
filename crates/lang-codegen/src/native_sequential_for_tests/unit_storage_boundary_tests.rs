//! Unit storage does not weaken the native analysis-identity/publication boundary.

use super::super::{
    NativeObjectErrorKind, SymbolKind, TestDirectory,
};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

#[test]
fn unit_storage_mixed_analysis_preserves_existing_object() {
    let first = UnitSource::analyze(
        "unit-capability.ko",
        r#"
        fun unit(): Unit { println("first") }
        fun main(): Unit { for (item in arrayOf<Unit>(unit())) { val copy: Unit = item } }
        "#,
    ).expect("original source must pass strict Unit admission");
    let second = UnitSource::analyze(
        "unit-capability.ko",
        r#"
        fun unit(): Unit { println("other") }
        fun main(): Unit { for (item in arrayOf<Unit>(unit())) { val copy: Unit = item } }
        "#,
    ).expect("original source must pass strict Unit admission");
    for analysis in [&first, &second] {
        analysis.assert_frontend_clean();
    }
    let directory = TestDirectory::create();
    let output = directory.join("preserved.o");
    std::fs::write(&output, b"previous object bytes").unwrap();
    for (typed, owned) in [
        (&first, &second),
        (&second, &second),
    ] {
        let error = first.emit_with_products_from(
            typed, owned, first.declaration("main", SymbolKind::Function), &output,
        )
        .expect_err("foreign Unit facts must fail before object publication");
        assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
        assert_eq!(std::fs::read(&output).unwrap(), b"previous object bytes");
    }
}
