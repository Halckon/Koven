use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitNullableWhenUseKind, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    parser::Expression,
    type_checking::{UnitExpressionId, UnitNullableWhenDomain, standard_environments},
};

pub(super) fn assert_partial_own_call(text: &str) {
    for rooted in [false, true] {
        let (sources, parsed, owned, _, typed) = super::nullable_unit::checked_with_evidence(
            text,
            "partial-own-call.ko",
            rooted,
            standard_environments(),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [when] = owned.nullable_whens() else {
            panic!("one original When")
        };
        let descriptor = when.descriptor();
        let [null, non_null] = descriptor.entries() else {
            panic!("null then else")
        };
        assert_eq!(
            (null.body_domain(), non_null.body_domain()),
            (
                UnitNullableWhenDomain::Null,
                UnitNullableWhenDomain::NonNull
            )
        );
        let [action] = when.uses() else {
            panic!("only the actual take owns x")
        };
        assert_eq!(action.kind(), UnitNullableWhenUseKind::Consume);
        assert_eq!(action.loan(), None);
        let [proof] = typed.types().nullable_when_uses() else {
            panic!("one actual typed witness")
        };
        assert_eq!(action.witness(), *proof);
        assert_eq!(proof.control(), descriptor.expression());
        assert_eq!(proof.subject(), descriptor.subject());
        assert_eq!(Some(proof.symbol()), descriptor.stable_symbol());
        assert_eq!(
            Some(proof.source_generation()),
            descriptor.source_generation()
        );
        assert_eq!((proof.entry_index(), proof.body()), (1, non_null.body()));
        let exprs = parsed.ast().expressions();
        let unit = descriptor.expression().source_unit();
        let span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), unit);
            exprs.get(id.expression()).unwrap().span()
        };
        let call_span = span(action.call());
        assert_eq!(sources.slice(call_span), Ok("take(x)"));
        assert_eq!(sources.slice(span(action.argument())), Ok("x"));
        assert_eq!(action.reference(), action.argument());
        assert_eq!(action.moved_span(), Some(span(action.reference())));
        let [delivery] = owned.value_deliveries() else {
            panic!("only one actual Value action")
        };
        assert_eq!(action.delivery(), Some(delivery));
        assert_eq!(
            (delivery.call(), delivery.argument()),
            (action.call(), action.argument())
        );
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
        assert_eq!(
            delivery.source(),
            &UnitValueDeliverySource::Place(when.source_place().unwrap().clone())
        );
        assert_eq!(
            sources.slice(delivery.parameter_span().unwrap()),
            Ok("own x: Node")
        );
        let nested_if = exprs
            .iter()
            .find_map(|(id, node)| {
                let Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                assert_eq!(
                    sources.slice(node.span()),
                    Ok("if (flag) { take(x) } else {}")
                );
                assert_eq!(
                    sources.slice(exprs.get(*condition).unwrap().span()),
                    Ok("flag")
                );
                let statements = parsed.ast().statements();
                let true_span = statements.get(*then_branch).unwrap().span();
                let false_span = statements.get(else_branch.unwrap()).unwrap().span();
                assert!(
                    true_span.start() <= call_span.start() && call_span.end() <= true_span.end()
                );
                assert!(call_span.end() <= false_span.start());
                assert_eq!(sources.slice(false_span), Ok("{}"));
                Some(UnitExpressionId::new(unit, id))
            })
            .expect("actual nested If");
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(proof.symbol()))
            .map(|drop| drop.point())
            .collect::<Vec<_>>();
        assert_eq!(
            drops,
            [
                UnitDropPoint::BranchExit {
                    control: descriptor.expression(),
                    branch: 0
                },
                UnitDropPoint::BranchExit {
                    control: nested_if,
                    branch: 1
                },
            ],
            "retire original root only when null or when the move was skipped"
        );
        assert!(owned.loans().is_empty());
        assert!(owned.rc_effects().is_empty());
        assert!(owned.validate().is_ok());
    }
}
