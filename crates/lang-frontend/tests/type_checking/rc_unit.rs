//! 原 Rc 构造、share、payload 与失败原子性消费者的双 Unit 入口。
use super::{codes, unit_entries::checked_entries};

#[test]
fn intrinsic_rc_publishes_construction_share_and_payload_borrow_facts() {
    use lang_frontend::type_checking::{
        Copyability, ParameterMode, RcOperationKind, UnitConstructionTarget,
    };

    let text = "value class Point(val x: Int)
                fun shared(): Int {
                    val first = Rc(Point(1))
                    val second = first.share()
                    val explicit = Rc<Point>(Point(2))
                    val copied = first.value.x
                    return second.value.x
                }";
    let (_, _, entries) = checked_entries(text);
    for (source, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let rc_constructions = typed
            .constructions()
            .iter()
            .filter(|descriptor| descriptor.target() == UnitConstructionTarget::IntrinsicRc)
            .collect::<Vec<_>>();
        assert_eq!(rc_constructions.len(), 2);
        assert!(rc_constructions.iter().all(|descriptor| {
            descriptor.arguments().len() == 1
                && descriptor.arguments()[0].parameter_name() == "value"
                && descriptor.arguments()[0].mode() == ParameterMode::Value
                && typed.copyability(descriptor.result_type()) == Copyability::MoveOnly
        }));
        assert_eq!(typed.rc_operations().len(), 3);
        assert_eq!(typed.rc_operations()[0].kind(), RcOperationKind::Share);
        assert_eq!(typed.rc_operations()[0].result_mode(), ParameterMode::Value);
        assert!(typed.rc_operations()[1..].iter().all(|descriptor| {
            descriptor.kind() == RcOperationKind::Value
                && descriptor.result_mode() == ParameterMode::Borrow
        }));
        assert!(
            typed
                .constructions()
                .iter()
                .all(|descriptor| { descriptor.expression().source_unit() == source })
        );
        assert!(typed.rc_operations().iter().all(|descriptor| {
            descriptor.expression().source_unit() == source
                && descriptor.receiver().source_unit() == source
        }));
    }
}

#[test]
fn rc_shapes_reuse_existing_call_diagnostics_without_partial_facts() {
    let text = "fun missing(): Unit { val result = Rc() }
                fun extra(): Unit { val result = Rc<Int>(1, 2) }
                fun badShare(): Unit {
                    val owner = Rc(1)
                    val shared = owner.share(2)
                }";
    let (_, _, entries) = checked_entries(text);
    for (source, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0121", "L0121", "L0121"]);
        assert_eq!(
            typed
                .constructions()
                .iter()
                .filter(|descriptor| matches!(
                    descriptor.target(),
                    lang_frontend::type_checking::UnitConstructionTarget::IntrinsicRc
                ))
                .count(),
            1
        );
        assert!(typed.rc_operations().is_empty());
        assert!(
            typed
                .constructions()
                .iter()
                .all(|descriptor| { descriptor.expression().source_unit() == source })
        );
        assert!(typed.rc_operations().iter().all(|descriptor| {
            descriptor.expression().source_unit() == source
                && descriptor.receiver().source_unit() == source
        }));
    }
}
