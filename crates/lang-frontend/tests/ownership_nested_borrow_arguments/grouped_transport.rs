use lang_frontend::{
    ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    parser::Expression,
    type_checking::UnitExpressionId,
};

pub(super) fn assert_grouped_same_root_transport() {
    let text = include_str!("fixtures/grouped-transport.ko");
    let c = super::unit_checked(text);
    for ((unit, unit_id), names) in [&c.memory, &c.rooted]
        .into_iter()
        .zip(c.source_units)
        .zip(&c.unit_names)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let expressions = c.parsed.ast().expressions();
        let node = |x: UnitExpressionId| {
            assert_eq!(x.source_unit(), unit_id);
            expressions.get(x.expression()).unwrap()
        };
        let spell = |x| c.sources.slice(node(x).span()).unwrap();
        let [second, first] = unit.borrow_results().call_uses() else {
            panic!("source-order uses")
        };
        let outer = second.call();
        assert_eq!(first.call(), outer);
        assert_eq!(
            spell(outer),
            "pair(second = ((view((source)))), first = view(source))"
        );
        let Expression::Call { arguments, .. } = node(outer).payload() else {
            panic!("actual named call")
        };
        let [second_arg, first_arg] = arguments.as_slice() else {
            panic!("two real source arguments")
        };
        assert_eq!(second.argument().expression(), second_arg.value);
        assert_eq!(first.argument().expression(), first_arg.value);
        assert_eq!(
            c.sources.slice(second_arg.named_prefix.unwrap().name_span),
            Ok("second")
        );
        assert_eq!(
            c.sources.slice(first_arg.named_prefix.unwrap().name_span),
            Ok("first")
        );
        assert_ne!(second.argument(), second.expression());
        assert_eq!(first.argument(), first.expression());
        assert_ne!(second.expression(), first.expression());
        assert_eq!(second.origin(), first.origin());
        let UnitLoanTarget::Place(root) = first.origin() else {
            panic!("same named root")
        };
        assert!(root.is_root());
        assert_eq!(root.root().source_unit(), unit_id);
        let symbol = names.names().source_units()[unit_id.index()]
            .resolution()
            .symbols()
            .get(root.root().symbol().index())
            .unwrap()
            .span();
        assert_eq!(c.sources.slice(symbol), Ok("source"));
        assert_eq!(
            symbol.start(),
            text.find("val source").unwrap() + "val ".len()
        );
        let [first_end, second_end] = unit.borrow_call_transports() else {
            panic!("two distinct transports")
        };
        for (usage, transport, parameter, arg, canonical, operand) in [
            (
                first,
                first_end,
                "first: String",
                "view(source)",
                "view(source)",
                "source",
            ),
            (
                second,
                second_end,
                "second: String",
                "((view((source))))",
                "view((source))",
                "(source)",
            ),
        ] {
            assert!(usage.parent().is_none());
            let [child, lender] = transport.ended_loans();
            assert_eq!(spell(usage.argument()), arg);
            assert_eq!(spell(usage.expression()), canonical);
            assert_eq!(spell(usage.source_loan().argument()), operand);
            assert_eq!(usage.source_loan().call(), usage.expression());
            assert_eq!((child.call(), child.argument()), (outer, usage.argument()));
            assert_eq!(
                (lender.call(), lender.argument()),
                (usage.expression(), usage.source_loan().argument())
            );
            assert_eq!(
                c.sources.slice(child.parameter_span().unwrap()),
                Ok(parameter)
            );
            assert_eq!(
                c.sources.slice(lender.parameter_span().unwrap()),
                Ok("source: String")
            );
            assert_eq!(lender.begin_span(), node(lender.argument()).span());
            assert_eq!(child.begin_span(), node(child.argument()).span());
            for loan in [child, lender] {
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert_eq!(loan.target(), usage.origin());
                assert!(unit.loans().contains(loan));
            }
            assert_eq!(
                transport.handoff_point(),
                UnitDropPoint::CallReturn(usage.expression())
            );
            assert_eq!(transport.point(), UnitDropPoint::CallReturn(outer));
            assert_eq!(usage.end_point(), transport.point());
        }
        for (a, b) in first_end
            .ended_loans()
            .into_iter()
            .zip(second_end.ended_loans())
        {
            assert_ne!(a, b);
        }
        assert!(unit.value_deliveries().is_empty());
        assert!(
            unit.drops()
                .iter()
                .all(|d| !matches!(d.target(), UnitDropTarget::Temporary(_)))
        );
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root.root()))
            .collect();
        let [drop] = drops.as_slice() else {
            panic!("one Drop of original root")
        };
        assert_eq!(drop.point(), UnitDropPoint::CallReturn(outer));
        assert_eq!(drop.value_origin(), symbol);
        assert!(unit.clone().validate().is_ok());
    }
}
