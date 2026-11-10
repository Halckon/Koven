//! Ordinary finite source programs for the separately declared Named argument protocol.
use super::*;
use lang_frontend::ownership_checking::UnitLoanActionCoverage;

#[test]
fn named_owned_argument_requires_actual_closed_region() {
    let observed = observe(
        "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [end] = unit.direct_call_loan_ends() else {
            panic!("actual old EndCallLoan");
        };
        let UnitLoanTarget::Place(place) = end.loan().target() else {
            panic!("named owner root");
        };
        assert!(place.is_root());
        assert_eq!(
            unit.drops()
                .iter()
                .filter(|drop| drop.point() == end.point()
                    && drop.target() == UnitDropTarget::Named(place.root()))
                .count(),
            1
        );
        let observation = &unit.loan_action_observations()[0];
        let [region] = observation.regions() else {
            panic!("actual entered region");
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.protocol_version(), 3);
        assert_eq!(region.action_ids().len(), 3);
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
    }
}
