//! Real Continue has its own source/target identity; it is not a normal fallthrough.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        continue
    }
    return held
}
fun entry(): Unit { val a = run(0); val b = run(1); val c = run(2); val d = run(7) }
"#;

#[test]
fn original_continue_requires_actual_header_return_authority() {
    let source = UnitSource::analyze("owner-continue-header.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = receipts.as_slice() else {
        panic!("one owned Return");
    };
    assert!(
        receipt.2.is_some(),
        "Continue header current owner lacks real Return authority"
    );
    assert_eq!(receipt.1, receipt.2);
}
