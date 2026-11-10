//! Finite real Boolean comparisons; no injected facts, foreign lookups or resource counters.
use super::{OwnedProducts, UnitSource};
const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(flag == true) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
// Whether the complete source condition is true when its parameter is false.
const CASES: &[(&str, bool)] = &[
    ("flag == true", false),
    ("flag == false", true),
    ("flag != true", true),
    ("flag != false", false),
    ("true == flag", false),
    ("false == flag", true),
    ("true != flag", true),
    ("false != flag", false),
    ("!((!flag) == true)", false),
    ("(true != (!flag))", false),
];
#[test]
fn boolean_comparison_actual_return_requires_owner_receipt() {
    let source = UnitSource::analyze("comparison-owner.ko", SOURCE).unwrap();
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
        "actual comparison Return needs P/Q authorization"
    );
}
#[test]
fn boolean_comparison_keeps_actual_owner_edges_in_both_units() {
    for (condition, _) in CASES {
        super::owner_return_tests::assert_return_transport(
            &SOURCE.replace("flag == true", condition),
        );
    }
}
#[test]
fn boolean_comparison_native_preserves_all_truth_tables() {
    let body = SOURCE.split("\nfun entry").next().unwrap();
    let mut text = String::new();
    let mut entry = String::from("fun entry(): Unit {");
    let mut expected = Vec::new();
    for (index, (condition, false_result)) in CASES.iter().enumerate() {
        text.push_str(
            &body
                .replace("fun run(", &format!("fun run{index}("))
                .replace("flag == true", condition),
        );
        text.push('\n');
        entry.push_str(&format!("val first{index} = run{index}(false); println(first{index}); val second{index} = run{index}(true); println(second{index});"));
        expected.extend_from_slice(if *false_result {
            b"next\nouter\n"
        } else {
            b"outer\nnext\n"
        });
    }
    entry.push('}');
    text.push_str(&entry);
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("(true)", "(ON)")
            )
        } else {
            text.clone()
        };
        let source = UnitSource::analyze("comparison-owner-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::assert_native_output(&source, &expected);
    }
}
