//! 剩余单 case 的 payload facts 不越过 nullable 域、来源版本或分支边界。
use super::{super::codes, checked_entries};
use lang_frontend::type_checking::{
    BuiltinType, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitTypeKind,
};

fn assert_payload(text: &str, access: &str, generic: bool) {
    let (sources, parsed, entries) = checked_entries(text);
    for (source, typed) in entries {
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
        let [projection] = typed.aggregate_projections() else {
            panic!("one uniquely selected payload: {text}");
        };
        assert_eq!(projection.expression().source_unit(), source);
        assert_eq!(projection.field().source_unit(), source);
        assert_eq!(projection.kind(), UnitAggregateProjectionKind::Field);
        assert_eq!(
            typed.types().get(projection.ty()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        let node = parsed
            .ast()
            .expressions()
            .get(projection.expression().expression())
            .unwrap();
        assert_eq!(sources.slice(node.span()), Ok(access));
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            panic!("explicit stable receiver");
        };
        assert_eq!(receiver.source_unit(), source);
        let Some(UnitTypeKind::EnumCase { case, root }) = typed
            .expression_type(receiver)
            .and_then(|ty| typed.types().get(ty))
        else {
            panic!("the receiver must keep the surviving enum case");
        };
        assert_eq!(case.source_unit(), source);
        let Some(UnitTypeKind::Nominal { arguments, .. }) = typed.types().get(*root) else {
            panic!("the refinement must retain its instantiated runtime root");
        };
        assert_eq!(arguments.len(), usize::from(generic));
        if generic {
            assert_eq!(
                typed.types().get(arguments[0]),
                Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            );
        }
        assert!(typed.validate().is_ok());
    }
}

fn assert_payload_rejected(text: &str, valid_payloads: usize) {
    let (sources, parsed, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0113"],
            "{text}: {:?}",
            typed.diagnostics()
        );
        let span = typed.diagnostics()[0].primary_span();
        assert_eq!(span.source_id(), parsed.source_id());
        assert_eq!(sources.slice(span), Ok("radius"));
        let start = text.rfind("radius").unwrap();
        assert_eq!(span.start(), start);
        assert_eq!(span.end(), start + "radius".len());
        let payloads = typed
            .aggregate_projections()
            .iter()
            .filter(|projection| {
                let node = parsed
                    .ast()
                    .expressions()
                    .get(projection.expression().expression())
                    .unwrap();
                sources
                    .slice(node.span())
                    .is_ok_and(|text| text.ends_with(".radius"))
            })
            .count();
        assert_eq!(
            payloads, valid_payloads,
            "failed payload must not publish a projection"
        );
        assert!(typed.validate().is_err());
    }
}

#[test]
fn remaining_case_payload_orders_keep_source_qualified_types() {
    for text in [
        "enum class Shape { Circle(radius: Int), Point }
        fun extract(x: Shape?): Int = when (x) {
            null -> 0; is Shape.Point -> 0; else -> x.radius
        }",
        "enum class Shape { Circle(radius: Int), Point }
        fun extract(x: Shape?): Int = when (x) {
            is Shape.Point -> 0; null -> 0; else -> x.radius
        }",
    ] {
        assert_payload(text, "x.radius", false);
    }
}

#[test]
fn remaining_case_payload_substitutes_generic_root_arguments() {
    let text = "enum class Maybe<T> { Some(item: T), None }
        fun extract(x: Maybe<Int>?): Int = when (x) {
            null -> 0; !is Maybe.None<Int> -> x.item; else -> 0
        }";
    assert_payload(text, "x.item", true);
}

#[test]
fn remaining_case_payload_requires_single_non_null_alternative() {
    for text in [
        "enum class Shape { Circle(radius: Int), Point }
        fun extract(x: Shape?): Int = when (x) {
            null, is Shape.Circle -> x.radius; else -> 0
        }",
        "enum class Shape { Circle(radius: Int), Square(side: Int), Point }
        fun extract(x: Shape?): Int = when (x) {
            null -> 0; !is Shape.Point -> x.radius; else -> 0
        }",
    ] {
        assert_payload_rejected(text, 0);
    }
}

#[test]
fn remaining_case_payload_rejects_changed_or_unstable_source() {
    for text in [
        "enum class Shape { Circle(radius: Int), Point }
        fun extract(input: Shape?, replacement: Shape?): Int {
            var x = input
            return (when (x) {
                null -> 0;
                if (true) { x = replacement; Shape.Point } else { Shape.Point } -> 0;
                is Shape.Point -> 0; else -> x.radius
            })
        }",
        "enum class Shape { Circle(radius: Int), Point }
        fun mutate(inout input: Shape?): Shape = Shape.Point
        fun extract(input: Shape?): Int {
            var x = input
            return (when (x) {
                mutate(&x) -> 0; null -> 0; is Shape.Point -> 0; else -> x.radius
            })
        }",
        "enum class Shape { Circle(radius: Int), Point }
        fun mutate(inout input: Shape?): Shape = Shape.Point
        fun extract(input: Shape?, candidate: Shape): Int {
            var x = input
            return (when (x) {
                null -> 0; is Shape.Point -> 0;
                candidate, mutate(&x) -> x.radius; else -> 0
            })
        }",
        "enum class Shape { Circle(radius: Int), Point }
        class Holder(val shape: Shape?)
        fun extract(h: Holder): Int = when (h.shape) {
            null -> 0; is Shape.Point -> 0; else -> h.shape.radius
        }",
        "enum class Shape { Circle(radius: Int), Point }
        fun extract(input: Shape?): Int {
            var x = input
            val capture: () -> Shape? = { x }
            return (when (x) { null -> 0; is Shape.Point -> 0; else -> x.radius })
        }",
        "enum class Shape { Circle(radius: Int), Point }
        fun subject(): Shape? = Shape.Point
        fun extract(): Int = when (subject()) {
            null -> 0; is Shape.Point -> 0; else -> subject().radius
        }",
    ] {
        assert_payload_rejected(text, 0);
    }
}

#[test]
fn remaining_case_payload_does_not_leak_across_body_writes_or_joins() {
    let changed = "enum class Shape { Circle(radius: Int), Point }
        fun extract(input: Shape?, replacement: Shape?): Int {
            var x = input
            return (when (x) {
                null -> 0; !is Shape.Point -> { x = replacement; x.radius }; else -> 0
            })
        }";
    assert_payload_rejected(changed, 0);
    let joined = "enum class Shape { Circle(radius: Int), Point }
        fun extract(x: Shape?): Int {
            when (x) { null -> 0; is Shape.Point -> 0; else -> x.radius }
            return x.radius
        }";
    assert_payload_rejected(joined, 1);
}

#[test]
fn remaining_case_payload_ignores_unselected_body_writes() {
    let text = "enum class Shape { Circle(radius: Int), Point }
        fun extract(input: Shape?, replacement: Shape?): Int {
            var x = input
            return (when (x) {
                null -> { x = replacement; 0 }; is Shape.Point -> 0; else -> x.radius
            })
        }";
    assert_payload(text, "x.radius", false);
}
