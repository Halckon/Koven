//! Ordinary pure Boolean leaf negation; actual products and original finite CFG only.
use super::{
    UnitSource,
    owner_short_circuit_tests::{SOURCE, check_cfg, check_native},
};
#[test]
fn leaf_not_actual_return_requires_owner_receipt() {
    let text = SOURCE.replace("flag && flag", "!flag && !flag");
    let source = UnitSource::analyze("leaf-not-owner.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, events) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [event] = events.as_slice() else {
        panic!("one actual Return")
    };
    assert!(event.1.is_some());
    assert_eq!(
        event.2, event.1,
        "actual leaf-Not Return needs P/Q authorization"
    );
}
#[test]
fn leaf_not_real_cfg_preserves_p_and_exit_roles() {
    check_cfg(&[
        "!flag && !flag",
        "!flag || !flag",
        "!((!flag) && (!flag))",
        "(!(!flag)) || flag",
        "!flag && flag",
        "flag || !flag",
    ]);
}
#[test]
fn leaf_not_native_preserves_short_circuit_truth_tables() {
    check_native(&[
        ("!flag && !flag", b"next\nouter\n"),
        ("!flag || !flag", b"next\nouter\n"),
        ("!((!flag) && (!flag))", b"outer\nnext\n"),
        ("(!(!flag)) || flag", b"outer\nnext\n"),
        ("!flag && flag", b"outer\nouter\n"),
        ("flag || !flag", b"next\nnext\n"),
    ]);
}
