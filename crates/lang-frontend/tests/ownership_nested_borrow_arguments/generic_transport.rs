use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    parser::Expression,
    type_checking::UnitExpressionId,
};

pub(super) fn assert_transport(text: &str) {
    let checked = super::unit_checked(text);
    let source = checked.parsed.source_id();
    assert_eq!(checked.sources.source_text(source), Ok(text));
    for ((unit, unit_id), names) in [&checked.memory, &checked.rooted]
        .into_iter()
        .zip(checked.source_units)
        .zip(&checked.unit_names)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let [usage] = unit.borrow_results().call_uses() else {
            panic!("one actual generic synchronous borrow")
        };
        let [transport] = unit.borrow_call_transports() else {
            panic!("one actual transport")
        };
        let [consumer, lender] = transport.ended_loans();
        let span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), unit_id);
            let span = checked
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span();
            assert_eq!(span.source_id(), source);
            span
        };
        let token = |id| checked.sources.slice(span(id)).unwrap();
        assert_eq!(token(lender.call()), "genericView(source)");
        assert_eq!(token(lender.argument()), "source");
        assert_eq!(token(consumer.call()), "observe(genericView(source))");
        assert_eq!(consumer.argument(), lender.call());
        assert_eq!(
            (usage.call(), usage.argument()),
            (consumer.call(), consumer.argument())
        );
        assert_eq!(usage.expression(), lender.call());
        assert_eq!(
            (usage.source_loan().call(), usage.source_loan().argument()),
            (lender.call(), lender.argument())
        );
        assert!(usage.parent().is_none());
        let UnitLoanTarget::Place(root) = usage.origin() else {
            panic!("named root")
        };
        assert!(root.is_root());
        let indexed = &names.names().source_units()[unit_id.index()];
        let symbol = indexed
            .resolution()
            .symbols()
            .get(root.root().symbol().index())
            .unwrap();
        assert_eq!(checked.sources.slice(symbol.span()), Ok("source"));
        assert_eq!(
            symbol.span().start(),
            text.find("val source").unwrap() + "val ".len()
        );
        assert_eq!(root.root().source_unit(), unit_id);
        assert_eq!(
            checked.sources.slice(lender.parameter_span().unwrap()),
            Ok("source: T")
        );
        assert_eq!(
            checked.sources.slice(consumer.parameter_span().unwrap()),
            Ok("item: String")
        );
        for loan in [consumer, lender] {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), usage.origin());
            assert!(unit.loans().contains(loan));
            let call_ast = checked
                .parsed
                .ast()
                .expressions()
                .get(loan.call().expression())
                .unwrap();
            let Expression::Call { arguments, .. } = call_ast.payload() else {
                panic!("actual call AST")
            };
            assert_eq!(arguments.len(), 1);
            assert_eq!(arguments[0].value, loan.argument().expression());
            assert_eq!(loan.begin_span(), span(loan.argument()));
            assert_eq!(loan.end_span(), span(loan.call()));
        }
        assert_eq!(
            transport.handoff_point(),
            UnitDropPoint::CallReturn(lender.call())
        );
        assert_eq!(
            transport.point(),
            UnitDropPoint::CallReturn(consumer.call())
        );
        assert_eq!(usage.end_point(), transport.point());
        assert!(unit.direct_call_loan_ends().is_empty());
        let [delivery] = unit.value_deliveries() else {
            panic!("one actual subsequent Move")
        };
        assert_eq!(token(delivery.call()), "consume(source)");
        assert_eq!(
            delivery.source(),
            &UnitValueDeliverySource::Place(root.clone())
        );
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
        assert!(span(consumer.call()).end() < span(delivery.argument()).start());
        assert!(unit.drops().iter().all(|fact| !matches!(
            fact.target(),
            UnitDropTarget::Temporary(_)
        ) && fact.target()
            != UnitDropTarget::Named(root.root())));
        assert!(unit.clone().validate().is_ok());
    }
}
