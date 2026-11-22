use lang_frontend::{
    ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    type_checking::UnitExpressionId,
};

#[test]
fn nullable_copy_ends_exact_source_after_snapshot_and_aux_on_inner_return() {
    let text = include_str!("fixtures/nullable-source-last-use.ko");
    let c = super::unit_checked(text);
    for ((unit, unit_id), names) in [&c.memory, &c.rooted]
        .into_iter()
        .zip(c.source_units)
        .zip(&c.unit_names)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), unit_id);
            c.parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span()
        };
        let spell = |id| c.sources.slice(span(id)).unwrap();
        let [usage] = unit.borrow_results().call_uses() else {
            panic!("actual result use")
        };
        assert!(usage.parent().is_none());
        let inner = usage.expression();
        assert_eq!(usage.argument(), inner);
        assert_eq!(spell(inner), "view(boxed, aux)");
        let outer = usage.call();
        assert_eq!(spell(outer), "observe(view(boxed, aux))");
        let source = usage.source_loan();
        assert_eq!(source.call(), inner);
        assert_eq!(spell(source.argument()), "boxed");
        let UnitLoanTarget::Place(root) = usage.origin() else {
            panic!("original named root")
        };
        assert!(root.is_root());
        let [source_loan, aux_loan, copied_use] = unit.loans() else {
            panic!("two named shared inputs and the distinct result use");
        };
        assert_eq!(
            (source_loan.call(), source_loan.argument()),
            (inner, source.argument())
        );
        assert_eq!(source_loan.target(), usage.origin());
        assert_eq!((copied_use.call(), copied_use.argument()), (outer, inner));
        assert_eq!(copied_use.target(), &UnitLoanTarget::Temporary(inner));
        assert_eq!(aux_loan.call(), inner);
        assert_eq!(spell(aux_loan.argument()), "aux");
        let UnitLoanTarget::Place(aux) = aux_loan.target() else {
            panic!("distinct auxiliary named root")
        };
        assert!(aux.is_root());
        assert_ne!(aux, root);
        for (loan, parameter) in [
            (source_loan, "source: Boxed"),
            (aux_loan, "aux: Boxed"),
            (copied_use, "value: Int?"),
        ] {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(
                c.sources.slice(loan.parameter_span().unwrap()),
                Ok(parameter)
            );
            assert_eq!(loan.begin_span(), span(loan.argument()));
        }
        let [aux_drop, source_drop] = unit.drops() else {
            panic!("exactly two actual owners retire")
        };
        let index = &names.names().source_units()[unit_id.index()];
        for (drop, root, name, point) in [
            (
                aux_drop,
                aux.root(),
                "aux",
                UnitDropPoint::CallReturn(inner),
            ),
            (
                source_drop,
                root.root(),
                "boxed",
                UnitDropPoint::AfterExpression(inner),
            ),
        ] {
            assert_eq!(root.source_unit(), unit_id);
            let symbol = index
                .resolution()
                .symbols()
                .get(root.symbol().index())
                .unwrap()
                .span();
            assert_eq!(c.sources.slice(symbol), Ok(name));
            assert_eq!(
                symbol.start(),
                text.find(&format!("val {name}")).unwrap() + "val ".len()
            );
            assert_eq!(drop.target(), UnitDropTarget::Named(root));
            assert_eq!(drop.point(), point);
            assert_eq!(drop.value_origin(), symbol);
            assert!(!drop.is_conditional_temporary());
        }
        assert_eq!(usage.end_point(), source_drop.point());
        assert!(unit.value_deliveries().is_empty());
        assert!(unit.clone().validate().is_ok());
    }
}
