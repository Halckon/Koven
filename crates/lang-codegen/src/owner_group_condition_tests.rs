//! Ordinary finite sources only. No injected facts, Abort, or counters.
use super::{OwnedProducts, UnitSource};
const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(((flag))) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
#[test]
fn grouped_condition_actual_return_requires_owner_receipt() {
    let source = UnitSource::analyze("grouped-owner.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, events) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [event] = events.as_slice() else {
        panic!("one real Return")
    };
    assert!(event.1.is_some());
    assert_eq!(
        event.2, event.1,
        "actual grouped Return needs P/Q authorization"
    );
}
#[test]
fn grouped_condition_preserves_both_units_real_edges_and_cleanup() {
    for condition in ["(flag)", "(((flag)))"] {
        let text = SOURCE.replace("((flag))", condition);
        super::owner_return_tests::assert_return_transport(&text);
    }
}
#[test]
fn grouped_condition_native_preserves_selected_value() {
    for constant in [false, true] {
        let text = SOURCE.replace(
            "val first = run(false); val second = run(true)",
            "val first = run(false); println(first); val second = run(true); println(second)",
        );
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("run(true)", "run(ON)")
            )
        } else {
            text
        };
        let source = UnitSource::analyze("grouped-owner-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::assert_native_output(&source, b"outer\nnext\n");
    }
}
