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

#[test]
fn named_root_stays_partial_until_its_actual_last_call_drop() {
    let observed = observe(
        "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source); inspect(source) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let [first, last] = observation.regions() else {
            panic!("two real regions");
        };
        assert_eq!(first.protocol_version(), 3);
        assert_eq!(first.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(first.action_ids().len(), 2);
        assert_eq!(last.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(last.action_ids().len(), 3);
        assert_eq!(unit.direct_call_loan_ends().len(), 2);
    }
}

#[test]
fn two_named_roots_keep_their_own_drop_formation_origins() {
    use lang_frontend::ownership_checking::{
        UnitIterationCleanupAction, UnitLoanAction, UnitLoanRegionFamily,
    };
    let observed = observe(
        "fun inspect(value: String): Int = 1\nfun run(own first: String, own second: String): Unit { inspect(first); inspect(value = second) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        assert_eq!(observation.regions().len(), 2);
        let mut roots = Vec::new();
        let mut origins = Vec::new();
        for region in observation.regions() {
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(
                region.families(),
                &[
                    UnitLoanRegionFamily::DirectSharedArgument,
                    UnitLoanRegionFamily::NamedSourceDrop
                ]
            );
            let actions: Vec<_> = region
                .action_ids()
                .map(|id| observation.action(&id).unwrap())
                .collect();
            let [
                UnitLoanAction::Begin(begin),
                UnitLoanAction::Cleanup {
                    action: UnitIterationCleanupAction::EndCallLoan(end),
                    ..
                },
                UnitLoanAction::Cleanup {
                    action: UnitIterationCleanupAction::Drop(drop),
                    ..
                },
            ] = actions.as_slice()
            else {
                panic!("whole real interval");
            };
            assert_eq!(begin, end);
            let UnitLoanTarget::Place(place) = begin.target() else {
                panic!("named source");
            };
            assert_eq!(drop.target(), UnitDropTarget::Named(place.root()));
            assert_ne!(drop.value_origin(), begin.begin_span());
            roots.push(place.root());
            origins.push(drop.value_origin());
        }
        assert_ne!(roots[0], roots[1]);
        assert_ne!(origins[0], origins[1]);
    }
}

#[test]
fn named_protocol_does_not_admit_other_sources_or_call_shapes() {
    for source in [
        "fun inspect(value: String): Int = 1\nfun run(source: String): Unit { inspect(source) }",
        "fun inspect(value: String): Int = 1\nfun run(inout source: String): Unit { inspect(source) }",
        "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect((source)) }",
        "fun inspect(value: String): Int = 1\nfun run(): Unit { val source = \"text\"; inspect(source) }",
        "fun inspect(value: String?): Int = 1\nfun run(own source: String): Unit { inspect(source) }",
        "fun inspect(first: String, second: String): Int = 1\nfun run(own source: String): Unit { inspect(source, source) }",
    ] {
        let observed = observe(source);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            let observation = &unit.loan_action_observations()[0];
            assert!(!observation.regions().is_empty());
            assert!(
                observation
                    .regions()
                    .iter()
                    .all(|region| region.coverage() != UnitLoanActionCoverage::Complete),
                "{source}"
            );
            assert_ne!(observation.coverage(), UnitLoanActionCoverage::Complete);
        }
    }
}

#[test]
fn named_certificate_and_records_do_not_cross_reanalysis() {
    let source = "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source) }";
    let left = observe(source);
    let right = observe(source);
    assert_units_admitted(&left);
    assert_units_admitted(&right);
    for (left, right) in left.units.iter().zip(&right.units) {
        let left = &left.loan_action_observations()[0];
        let right = &right.loan_action_observations()[0];
        let region = &left.regions()[0];
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert!(left.owns_region(region));
        assert!(!right.owns_region(region));
        for id in region.action_ids() {
            assert!(right.action(&id).is_none());
        }
    }
}

#[test]
fn named_unit_result_preserves_the_same_closed_obligations() {
    let observed = observe(
        "fun inspect(value: String): Unit {}\nfun run(own source: String): Unit { inspect(value = source) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let [region] = observation.regions() else {
            panic!("one actual region");
        };
        assert_eq!(region.protocol_version(), 3);
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.action_ids().len(), 3);
        assert_eq!(unit.direct_call_loan_ends().len(), 1);
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
    }
}
