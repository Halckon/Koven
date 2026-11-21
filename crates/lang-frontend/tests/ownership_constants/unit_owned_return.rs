use lang_frontend::ownership_checking::{
    ConstantMaterializationKind, LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    UnitValueDeliveryKind, UnitValueDeliverySource,
};

#[test]
fn original_owned_early_return_keeps_constant_and_literal_cleanup_equivalent() {
    let exact_constant = include_str!("fixtures/owned-constant-return.ko");
    let exact_literal = include_str!("fixtures/owned-literal-return.ko");
    assert_eq!(
        exact_constant.replace("take(TEXT,", "take(\"hi\","),
        exact_literal
    );
    for rooted in [false, true] {
        for (text, token) in [(exact_constant, "TEXT"), (exact_literal, "\"hi\"")] {
            let (sources, typed, owned) = super::unit_materialization::checked(text, rooted);
            assert!(owned.is_compatible_with(&typed));
            let inner = owned.ownership();
            let [delivery] = inner.value_deliveries() else {
                panic!("exact own delivery")
            };
            let value = delivery.argument();
            assert_eq!(delivery.kind(), UnitValueDeliveryKind::Temporary);
            assert_eq!(
                delivery.source(),
                &UnitValueDeliverySource::Temporary(value)
            );
            assert_eq!(sources.slice(delivery.span()), Ok(token));
            assert_eq!(
                sources.slice(delivery.parameter_span().unwrap()),
                Ok("own text: String")
            );
            if text == exact_constant {
                let [plan] = owned.materializations() else {
                    panic!("one String materialization")
                };
                assert_eq!(plan.descriptor().expression(), value);
                assert_eq!(plan.kind(), ConstantMaterializationKind::StringTemporary);
                assert!(owned.materialization_at(value).is_some());
            } else {
                assert!(owned.materializations().is_empty());
            }
            let [callee, early] = inner.drops() else {
                panic!("one caller and one callee release")
            };
            assert!(matches!(callee.point(), UnitDropPoint::FunctionEntry(_)));
            assert!(matches!(callee.target(), UnitDropTarget::Named(_)));
            assert_eq!(sources.slice(callee.value_origin()), Ok("text"));
            assert!(matches!(early.point(), UnitDropPoint::ControlTransfer(_)));
            assert_eq!(early.target(), UnitDropTarget::Temporary(value));
            assert_eq!(early.value_origin(), delivery.span());
            assert!(!early.is_conditional_temporary());
            let [scalar] = inner.loans() else {
                panic!("only the second scalar argument is borrowed")
            };
            assert_eq!(scalar.call(), delivery.call());
            assert_eq!(scalar.kind(), LoanKind::Shared);
            assert_eq!(
                scalar.target(),
                &UnitLoanTarget::Temporary(scalar.argument())
            );
            assert_eq!(
                sources.slice(scalar.begin_span()),
                Ok("if (flag) { return } else { 0 }")
            );
            assert_eq!(
                sources.slice(scalar.parameter_span().unwrap()),
                Ok("number: Int")
            );
        }
    }
}
