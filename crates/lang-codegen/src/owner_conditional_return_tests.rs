//! Conditional Return and implicit normal backedge require distinct owner authority.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int, stop: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        if(index < stop) { return held }
    }
    return held
}
fun entry(): Unit { val a = run(0, 2); val b = run(1, 2); val c = run(7, 2); val d = run(2, 7) }
"#;

#[test]
fn conditional_return_requires_both_actual_return_authorities() {
    let source = UnitSource::analyze("owner-conditional-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("real conditional Return source must lower");
    crate::ssa::verify::verify_program(&program).expect("real SSA must verify");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("verified SSA must render LLVM");
    let [conditional, tail] = receipts.as_slice() else {
        panic!("exactly two actual owned Returns, got {receipts:?}");
    };
    assert_ne!(conditional.0, tail.0, "distinct source Return identities");
    for (role, receipt) in [("conditional", conditional), ("tail", tail)] {
        assert!(
            receipt.1.is_some(),
            "{role} Return must have a real payload"
        );
        assert!(
            receipt.2.is_some(),
            "{role} Return lacks actual owner authority"
        );
        assert_eq!(
            receipt.1, receipt.2,
            "{role} authority must match its own payload"
        );
    }
}
