//! One immutable alias carries the same selected P/Q payload through a normal Return.
use super::UnitSource;

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    val forwarded = held
    return forwarded
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn alias_return_actual_payload_requires_product_authorization() {
    let source = UnitSource::analyze("owner-alias-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, returns) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) =
        lowered.expect("actual selected alias payload reaches old Return emitter");
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = returns.as_slice() else {
        panic!("one explicit Return");
    };
    assert!(receipt.1.is_some());
    assert_eq!(receipt.1, receipt.2);
}
