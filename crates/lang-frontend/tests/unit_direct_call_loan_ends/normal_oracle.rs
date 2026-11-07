//! Fixed source-byte oracle for bounded normal-return handoff observations.
use super::*;

// Call, original argument, declared parameter, and owned root declaration.
type LoanBytes = (ByteSpan, ByteSpan, ByteSpan, ByteSpan);

pub(super) fn assert_normal_handoff(
    observed: &Observed,
    binding: ByteSpan,
    source: LoanBytes,
    auxiliary: &[LoanBytes],
) {
    super::borrow_handoff::assert_handoff_identity(observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source unit")
        };
        let [local] = names.names().source_units() else {
            panic!("same-round local resolution")
        };
        let bytes = |span: Span| {
            assert_eq!(span.source_id(), observed.parsed.source_id());
            (span.start(), span.end())
        };
        let expression_span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), indexed.id());
            observed
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span()
        };
        let normalize = |loan: &lang_frontend::ownership_checking::UnitLoanFact| {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert!(unit.loans().contains(loan));
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("owned root loan")
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), indexed.id());
            assert!(unit.formed_owned_locals().contains(&place.root()));
            let root = &local.resolution().symbols()[place.root().symbol().index()];
            assert_eq!(root.id(), place.root().symbol());
            let call = expression_span(loan.call());
            let argument = expression_span(loan.argument());
            assert_eq!(loan.begin_span(), argument);
            assert_eq!(loan.end_span(), call);
            (
                bytes(call),
                bytes(argument),
                bytes(loan.parameter_span().unwrap()),
                bytes(root.span()),
            )
        };
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("complete singleton handoff vector")
        };
        assert_eq!(normalize(handoff.source_loan()), source);
        assert_eq!(handoff.binding().source_unit(), indexed.id());
        let symbol = &local.resolution().symbols()[handoff.binding().symbol().index()];
        assert_eq!(bytes(symbol.span()), binding);
        assert_eq!(
            handoff.point(),
            UnitDropPoint::CallReturn(handoff.source_loan().call())
        );
        assert_eq!(
            handoff
                .auxiliary_loan_ends()
                .iter()
                .map(normalize)
                .collect::<Vec<_>>(),
            auxiliary
        );
        assert!(
            handoff
                .auxiliary_loan_ends()
                .iter()
                .all(|loan| loan.argument() != handoff.source_loan().argument())
        );
        assert!(unit.borrow_call_prefix_ends().is_empty());
        assert!(
            unit.direct_call_loan_ends()
                .iter()
                .all(|event| event.loan().call() != handoff.source_loan().call())
        );
    }
}
