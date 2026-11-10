//! Finite legal sources only; Not parity changes runtime condition, never P/Q edge roles.
use super::{OwnedProducts, UnitSource};
const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(!flag) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
#[test]
fn negated_condition_actual_return_requires_owner_receipt() {
    let source = UnitSource::analyze("negated-owner.ko", SOURCE).unwrap();
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
        "actual negated Return needs P/Q authorization"
    );
}
#[test]
fn negated_condition_keeps_actual_false_and_break_roles_in_both_units() {
    for condition in ["!flag", "!(!flag)", "!((!(!flag)))"] {
        super::owner_return_tests::assert_return_transport(&SOURCE.replace("!flag", condition));
    }
}
#[test]
fn negated_condition_native_follows_actual_boolean_result() {
    for (condition, output) in [
        ("!flag", b"next\nouter\n".as_slice()),
        ("!(!flag)", b"outer\nnext\n".as_slice()),
        ("!((!(!flag)))", b"next\nouter\n".as_slice()),
    ] {
        for constant in [false, true] {
            let text = SOURCE.replace("!flag", condition).replace(
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
            let source = UnitSource::analyze("negated-owner-native.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert_eq!(
                matches!(source.products, OwnedProducts::Constant(..)),
                constant
            );
            super::assert_native_output(&source, output);
        }
    }
}
