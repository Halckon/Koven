//! Signature 的 case TypeRef 必须拒绝，合法 type-test 仍保留。
use super::{codes, diagnostic_spans, unit_entries::checked_entries};

#[test]
fn enum_case_signature_rejects_original_single_source_in_both_entries() {
    let text = "enum class Shape {\n\
                    Circle(radius: Int), Point;\n\
                    fun isCircle(): Boolean = this is Circle\n\
                }\n\
                fun invalid(input: Shape.Circle): Unit {}";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0114"], "{text}");
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[0]),
            ("Shape.Circle", "Shape")
        );
        assert!(typed.validate().is_err());
    }
}

#[test]
fn enum_case_types_are_rejected_through_nested_signature_positions() {
    for (declarations, primary) in [
        ("class Holder(val value: Shape.Circle)", "Shape.Circle"),
        ("fun invalid(): Shape.Circle", "Shape.Circle"),
        ("fun invalid(value: Shape.Circle?): Unit", "Shape.Circle?"),
        (
            "fun invalid(callback: (Shape.Circle) -> Unit): Unit",
            "Shape.Circle",
        ),
        (
            "class Wrapper<T> {}\nfun invalid(value: Wrapper<Shape.Circle>): Unit",
            "Shape.Circle",
        ),
        (
            "enum class Other { Payload(value: Shape.Circle) }",
            "Shape.Circle",
        ),
        ("class Invalid : Shape.Circle", "Shape.Circle"),
        ("fun <T: Shape.Circle> invalid(): Unit", "Shape.Circle"),
    ] {
        let text = format!("enum class Shape {{ Circle }}\n{declarations}");
        let (sources, _, entries) = checked_entries(&text);
        for (_, typed) in entries {
            assert_eq!(codes(typed.diagnostics()), ["L0114"], "{text}");
            assert_eq!(
                diagnostic_spans(&sources, &typed.diagnostics()[0]),
                (primary, "Shape")
            );
            assert!(typed.validate().is_err());
        }
    }
}

#[test]
fn enum_root_signatures_and_case_type_tests_remain_valid() {
    let text = "enum class Shape { Circle(radius: Int), Point }\n\
        fun inspect(shape: Shape): Boolean = shape is Shape.Circle\n\
        fun notPoint(shape: Shape): Boolean = shape !is Shape.Point";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.validate().is_ok());
    }
}
