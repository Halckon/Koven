//! Fixed source-byte identities for the complete bounded Return-prefix vector.
//! This does not claim a total order with other loan-end slices.
use super::*;

// Original call, argument, parameter, owned-root declaration and Return.
type PrefixBytes = (ByteSpan, ByteSpan, ByteSpan, ByteSpan, ByteSpan);

pub(super) fn assert_prefix_identities(observed: &Observed, expected: &[PrefixBytes]) {
    super::borrow_handoff::assert_handoff_identity(observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source unit")
        };
        let [local] = names.names().source_units() else {
            panic!("same-round resolution")
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
        let actual: Vec<_> = unit
            .borrow_call_prefix_ends()
            .iter()
            .map(|event| {
                let loan = event.loan();
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert!(unit.loans().contains(loan));
                let UnitLoanTarget::Place(place) = loan.target() else {
                    panic!("owned root source")
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
                let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                    panic!("actual Return edge")
                };
                let transfer_span = expression_span(transfer);
                assert_eq!(observed.sources.slice(transfer_span), Ok("return"));
                assert_eq!(event.preceding_drops(), 0);
                (
                    bytes(call),
                    bytes(argument),
                    bytes(loan.parameter_span().unwrap()),
                    bytes(root.span()),
                    bytes(transfer_span),
                )
            })
            .collect();
        assert_eq!(actual, expected);
    }
}

pub(super) fn assert_return_prefixes(observed: &Observed, expected: &[PrefixBytes]) {
    assert_prefix_identities(observed, expected);
    for unit in &observed.units {
        for event in unit.borrow_call_prefix_ends() {
            let UnitLoanTarget::Place(place) = event.loan().target() else {
                panic!("owned root source")
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.point() == event.point())
                .map(|drop| drop.target())
                .collect();
            assert_eq!(drops, vec![UnitDropTarget::Named(place.root())]);
        }
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_call_handoffs().is_empty());
        assert!(unit.direct_call_loan_ends().is_empty());
    }
}
