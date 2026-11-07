//! 原 enum payload 与 nullable flow 消费者，保留原源码和 smart-cast 事实。
use super::checked_entries;
use lang_frontend::type_checking::{UnitExpressionId, UnitTypeKind};

#[test]
fn type_tests_drive_enum_payload_and_nullable_smart_casts() {
    let text = "enum class Shape { Circle(radius: Int), Point }\n\
                fun radius(shape: Shape): Int {\n\
                    if (shape is Shape.Circle) { return shape.radius }\n\
                    return 0\n\
                }\n\
                fun increment(input: Int?): Int {\n\
                    if (input != null) { return input + 1 }\n\
                    return 0\n\
                }";
    let (sources, parsed, entries) = checked_entries(text);
    for (source, _, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.aggregate_projections().len(), 1);
        let [non_null_use] = typed.non_null_uses() else {
            panic!("expected exactly one proven non-null use");
        };
        assert_eq!(non_null_use.expression().source_unit(), source);
        assert_eq!(non_null_use.symbol().source_unit(), source);
        let expression = parsed
            .ast()
            .expressions()
            .get(non_null_use.expression().expression())
            .expect("non-null expression");
        assert_eq!(sources.slice(expression.span()), Ok("input"));
        assert_eq!(
            typed.types().get(non_null_use.declared_type()),
            Some(&UnitTypeKind::Nullable(non_null_use.narrowed_type()))
        );
        assert_eq!(
            typed.expression_type(non_null_use.expression()),
            Some(non_null_use.narrowed_type())
        );
        assert_eq!(
            typed.non_null_use(non_null_use.expression()),
            Some(*non_null_use)
        );
        let comparison = typed
            .null_comparison(UnitExpressionId::new(
                source,
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find(|(_, node)| sources.slice(node.span()) == Ok("input != null"))
                    .expect("null comparison")
                    .0,
            ))
            .expect("typed null comparison fact");
        assert!(comparison.non_null_when_true());
        assert_eq!(comparison.symbol(), non_null_use.symbol());
        assert_eq!(comparison.nullable_type(), non_null_use.declared_type());
    }
}

#[test]
fn exhaustive_when_covers_enum_boolean_nullable_and_short_circuit_facts() {
    let text = "enum class Shape {
                    Circle(radius: Int), Point;
                    fun ownRadius(): Int = when (this) {
                        is Circle -> radius
                        is Point -> 0
                    }
                }
                fun external(shape: Shape): Int = when (shape) {
                    is Shape.Circle -> shape.radius
                    Shape.Point -> 0
                }
                fun positive(shape: Shape): Boolean =
                    shape is Shape.Circle && shape.radius > 0
                fun local(shape: Shape): Boolean {
                    val current = shape
                    return current is Shape.Circle && current.radius > 0
                }
                fun disjunction(shape: Shape): Boolean =
                    shape !is Shape.Circle || shape.radius > 0
                fun afterExit(shape: Shape): Int {
                    if (shape !is Shape.Circle) { return 0 }
                    return shape.radius
                }
                fun nullable(shape: Shape?): Int = when (shape) {
                    null -> 0
                    is Shape.Circle -> shape.radius
                    is Shape.Point -> 0
                }
                fun boolean(flag: Boolean): Int = when (flag) {
                    true -> 1
                    false -> 0
                }
                fun preserveRoot(shape: Shape): Shape {
                    if (shape is Shape.Circle) { return shape }
                    return shape
                }";
    let (_, _, entries) = checked_entries(text);
    for (_, _, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    }
}

#[test]
fn generic_enum_case_tests_substitute_root_arguments_for_payloads() {
    let text = "enum class Maybe<T> {
                    Some(item: T), None;
                    fun valueOr(fallback: T): T = when (this) {
                        is Some -> item
                        is None -> fallback
                    }
                }
                fun external(input: Maybe<Int>): Int = when (input) {
                    is Maybe.Some<Int> -> input.item
                    is Maybe.None<Int> -> 0
                }";
    let (_, _, entries) = checked_entries(text);
    for (_, _, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    }
}
