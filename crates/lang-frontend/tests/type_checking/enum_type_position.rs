//! Signature 的 case TypeRef 必须拒绝，合法 type-test 仍保留。
use super::{codes, diagnostic_spans, unit_entries::checked_entries};
use lang_frontend::type_checking::{BuiltinType, UnitTypeKind, UnitTypeRefId};

#[test]
fn enum_case_signature_rejects_original_single_source_in_both_entries() {
    let text = "enum class Shape {\n\
                    Circle(radius: Int), Point;\n\
                    fun isCircle(): Boolean = this is Circle\n\
                }\n\
                fun invalid(input: Shape.Circle): Unit {}";
    let (sources, parsed, entries) = checked_entries(text);
    for (source, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0114"], "{text}");
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[0]),
            ("Shape.Circle", "Shape")
        );
        let shape = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.nominal())
            .find(|nominal| !nominal.enum_cases().is_empty())
            .expect("Shape signature");
        let [circle, point] = shape.enum_cases() else {
            panic!("Shape has two enum cases");
        };
        assert_eq!((circle.name(), point.name()), ("Circle", "Point"));
        let [radius] = circle.payloads() else {
            panic!("Circle has one payload");
        };
        assert_eq!(radius.name(), "radius");
        assert_eq!(
            typed.types().get(radius.ty()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert!(point.payloads().is_empty());

        let (reference, _) = parsed
            .ast()
            .type_refs()
            .iter()
            .find(|(_, node)| sources.slice(node.span()) == Ok("Circle"))
            .expect("unqualified case in the member type test");
        let case_type = typed
            .type_ref_type(UnitTypeRefId::new(source, reference))
            .expect("type-test case type");
        assert_eq!(case_type, circle.case_type());
        assert_eq!(circle.type_symbol().source_unit(), source);
        assert_eq!(
            typed.types().get(case_type),
            Some(&UnitTypeKind::EnumCase {
                case: circle.type_symbol(),
                root: shape.ty(),
            })
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
        // Exercise the integration: case identity must exist before signatures,
        // and explicit visibility must still reach the real enum layout.
        for visibility in ["", "public "] {
            let shape = format!("{visibility}enum class Shape {{ Circle }}");
            for text in [
                format!("{shape}\n{declarations}"),
                format!("{declarations}\n{shape}"),
            ] {
                let (sources, file, entries) = checked_entries(&text);
                for (_, typed) in entries {
                    assert_eq!(codes(typed.diagnostics()), ["L0114"], "{text}");
                    let diagnostic = &typed.diagnostics()[0];
                    assert_eq!(diagnostic_spans(&sources, diagnostic), (primary, "Shape"));
                    assert_eq!(diagnostic.primary_span().source_id(), file.source_id());
                    assert_eq!(
                        diagnostic.primary_span().start(),
                        text.find(primary).unwrap()
                    );
                    assert!(typed.validate().is_err());
                }
            }
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
