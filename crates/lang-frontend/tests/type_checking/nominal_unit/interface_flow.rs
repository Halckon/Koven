//! 原静态接口、super receiver 与返回流消费者，不扩大 runtime 接口能力。
use super::super::{codes, diagnostic_spans};
use super::checked_entries;
use lang_frontend::{
    parser::Expression,
    type_checking::{
        UnitCallReceiverOrigin, UnitExpressionId, UnitTypeKind, UnitTypeParameterBound,
        UnitTypeRefId,
    },
};

#[test]
fn bounds_are_static_but_interfaces_are_not_runtime_value_types() {
    let text = "interface Protocol {}\n\
                class Concrete {}\n\
                class Good<T: Protocol> : Protocol {}\n\
                class AnyBound<T: Any> {}\n\
                class CopyBound<T: Copyable> {}\n\
                class BadClass<T: Concrete> {}\n\
                class BadNullable<T: Protocol?> {}\n\
                fun bad(input: Protocol): Unit";
    let (sources, _, entries) = checked_entries(text);
    for (_, _, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0092", "L0092", "L0094"]);
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[0]),
            ("Concrete", "T")
        );
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[1]),
            ("Protocol?", "T")
        );
        assert_eq!(
            diagnostic_spans(&sources, &typed.diagnostics()[2]),
            ("Protocol", "Protocol")
        );
        assert!(
            typed
                .signatures()
                .type_parameters()
                .values()
                .any(|parameter| matches!(parameter.bound(), UnitTypeParameterBound::Interface(_)))
        );
    }
}

#[test]
fn super_interface_member_uses_a_static_type_position() {
    let text = "interface Left { fun ping(): Int = 1 }\n\
                interface Right { fun ping(): Int = 2 }\n\
                class Resolved : Left, Right {\n\
                    override fun ping(): Int {\n\
                        val left = super<Left>.ping()\n\
                        val right = super<Right>.ping()\n\
                        return left + right\n\
                    }\n\
                }";
    let (_, parsed, entries) = checked_entries(text);
    for (source, _, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let super_members = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| match node.payload() {
                Expression::SuperMember { interface, .. } => Some((id, *interface)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(super_members.len(), 2);
        for (expression, interface) in super_members {
            assert!(matches!(
                typed
                    .expression_type(UnitExpressionId::new(source, expression))
                    .and_then(|ty| typed.types().get(ty)),
                Some(UnitTypeKind::Function { .. })
            ));
            assert!(matches!(
                typed
                    .type_ref_type(UnitTypeRefId::new(source, interface))
                    .and_then(|ty| typed.types().get(ty)),
                Some(UnitTypeKind::Nominal { .. })
            ));
        }
        assert_eq!(
            typed
                .calls()
                .iter()
                .filter(|call| {
                    matches!(
                        call.receiver().map(|receiver| receiver.origin()),
                        Some(UnitCallReceiverOrigin::ImplicitThis(_))
                    )
                })
                .count(),
            2
        );
    }
}

#[test]
fn super_interface_requires_inheritance_and_receiver_capability() {
    let (_, _, entries) = checked_entries(
        "interface Mutable { inout fun touch(): Unit {} }
         class Bad : Mutable { fun run(): Unit { super<Mutable>.touch() } }",
    );
    for (_, _, capability) in entries {
        assert_eq!(codes(capability.diagnostics()), ["L0100"]);
    }

    let (_, _, entries) = checked_entries(
        "interface Other { fun ping(): Unit {} }
         class Bad { fun run(): Unit { super<Other>.ping() } }",
    );
    for (_, _, unrelated) in entries {
        assert_eq!(codes(unrelated.diagnostics()), ["L0099"]);
    }

    let (_, _, entries) = checked_entries(
        "interface Mixed {\n\
             inout fun choose(input: Int): Int = 1\n\
             fun choose(input: String): Int = 2\n\
         }\n\
         class Good : Mixed { fun run(): Int = super<Mixed>.choose(\"ok\") }",
    );
    for (_, _, overloaded) in entries {
        assert!(
            overloaded.diagnostics().is_empty(),
            "{:?}",
            overloaded.diagnostics()
        );
    }

    let (_, _, entries) = checked_entries(
        "interface Base { fun ping(): Int = 1 }\n\
         interface Child : Base { fun run(): Int = super<Base>.ping() }",
    );
    for (_, _, interface_default) in entries {
        assert!(
            interface_default.diagnostics().is_empty(),
            "{:?}",
            interface_default.diagnostics()
        );
    }
}

#[test]
fn expression_tail_fallthrough_preserves_nullable_and_diverging_paths() {
    let text = "fun missing(input: Int?): Int { input ?: return 0 }\n\
                fun closed(input: Nothing?): Int { input ?: return 0 }\n\
                fun casted(): Int { (return 1) as Int }\n\
                fun propagated(): Int { (return 1)? }\n\
                fun referenced(): Int { (return 1)::next }";
    let (_, _, entries) = checked_entries(text);
    for (_, _, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0088"]);
    }
}
