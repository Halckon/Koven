//! Constructed missing or forged evidence is CI-only, never run locally.
use super::super::super::borrow_forward_transport::ForwardCause;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{UnitBorrowForwardTransportFact, UnitLoanTarget};

fn recorded(fact: &UnitBorrowForwardTransportFact, scope: ForwardScope) -> ActionSource {
    let mut actions = ActionSource::default();
    let scope = actions.enter_forward_region(scope);
    let acquisition = actions.begin(fact.source_loan().clone()).unwrap();
    actions.forward_handoff(
        fact.clone(),
        Some(ForwardCause { acquisition, scope }),
        Some(scope),
    );
    actions.complete_forward_region(Some(scope), true, true);
    actions
}

#[test]
fn ci_only_forward_region_requires_all_boundaries_and_same_catalog() {
    let fixture = Fixture::new(&[(
        "a.ko",
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)",
    )]);
    let owned = fixture.shared();
    let fact = &owned.borrow_forward_transports()[0];
    let loan = fact.source_loan();
    let wrap = fixture
        .names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "wrap")
        .unwrap();
    let UnitLoanTarget::Place(place) = loan.target() else {
        panic!("root")
    };
    let scope = ForwardScope {
        region: None,
        callable: UnitItemId::new(wrap.source_unit(), wrap.root()),
        body: loan.call(),
        control: loan.call(),
        branch: None,
        branch_entry: None,
        argument: loan.argument(),
        root: place.root(),
    };
    let check = |actions: &ActionSource| {
        let links = super::super::forward_links::validate(
            &actions.records,
            &actions.identity,
            loan.call().source_unit(),
            fixture.parsed[0].source_id(),
            &actions.branch_entries,
        );
        actions.validate_forward_regions(&links)
    };
    assert_eq!(
        check(&recorded(fact, scope))[0].coverage(),
        UnitLoanActionCoverage::Complete
    );
    for fault in 0..11 {
        let mut actions = recorded(fact, scope);
        match fault {
            0 => actions.forward_regions[0].end = None,
            1 => actions.forward_regions[0].pre_clear = false,
            2 => actions.forward_regions[0].post_clear = false,
            3 => {
                let actual = actions.forward_regions[0].scope;
                actions.complete_forward_region(Some(actual), true, true);
            }
            4 => {
                actions.begin(loan.clone());
                actions.forward_regions[0].end = Some(actions.records.len());
            }
            5 => actions.forward_regions[0].handoff = None,
            6 => actions.forward_regions[0].handoff = ActionSource::default().begin(loan.clone()),
            7 => actions.forward_regions[0].handoff.as_mut().unwrap().index = 0,
            8 => {
                let actual = actions.forward_regions[0].scope;
                let handle = actions.forward_regions[0].handoff.clone().unwrap();
                actions.record_forward_region_handoff(Some(actual), handle);
            }
            9 => actions.forward_regions[0].start = 1,
            _ => actions.forward_regions[0].scope.body = scope.argument,
        }
        let regions = check(&actions);
        assert_eq!(regions.len(), 1);
        assert_eq!(
            regions[0].coverage(),
            UnitLoanActionCoverage::Partial,
            "fault {fault}"
        );
        assert!(regions[0].live_handoff().is_none(), "fault {fault}");
    }
}
