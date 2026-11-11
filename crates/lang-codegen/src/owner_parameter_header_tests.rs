//! Owned parameter entry must retain its identity through a changing normal header.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int, own incoming: String): String {
    var held = incoming
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
    }
    return held
}
fun entry(): Unit { val a = run(0, "outer"); val b = run(1, "outer"); val c = run(2, "outer"); val d = run(7, "outer") }
"#;

#[test]
fn owned_parameter_normal_header_requires_actual_return_authority() {
    let source = UnitSource::analyze("owner-parameter-header.ko", SOURCE)
        .expect("ordinary owned parameter source must analyze");
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("owned parameter normal loop must lower");
    crate::ssa::verify::verify_program(&program).expect("real SSA must verify");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("verified SSA must render LLVM");
    let [receipt] = receipts.as_slice() else {
        panic!("exactly one actual owned Return, got {receipts:?}");
    };
    assert!(receipt.1.is_some(), "Return must have an actual payload");
    assert!(
        receipt.2.is_some(),
        "owned parameter header lacks actual Return authority"
    );
    assert_eq!(
        receipt.1, receipt.2,
        "authority must match the actual payload"
    );
}
