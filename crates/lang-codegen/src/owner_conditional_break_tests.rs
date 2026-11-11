//! Conditional Break and implicit normal backedge require distinct owner authority.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int, stop: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        if(index < stop) { break }
    }
    return held
}
fun entry(): Unit { val a = run(0, 2); val b = run(1, 2); val c = run(7, 2); val d = run(2, 7) }
"#;

#[test]
fn conditional_break_requires_actual_exit_return_authority() {
    let source = UnitSource::analyze("owner-conditional-break.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("real conditional Break source must lower");
    crate::ssa::verify::verify_program(&program).expect("real SSA must verify");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("verified SSA must render LLVM");
    let [receipt] = receipts.as_slice() else {
        panic!("one owned Return");
    };
    assert!(
        receipt.2.is_some(),
        "conditional Break and normal backedge lack real Return authority"
    );
    assert_eq!(receipt.1, receipt.2);
}
