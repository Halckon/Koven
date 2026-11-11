//! Broken certificates are CI-only; compile these guards, never run them locally.
use super::super::super::borrow_forward_transport::ForwardCause;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{
    UnitBorrowForwardTransportFact, UnitLoanAction, UnitLoanForwardBranch, UnitLoanTarget,
};
use crate::{name_resolution::SourceUnitId, source::SourceId};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view(source) }; return view(source) }";

fn scope(fact: &UnitBorrowForwardTransportFact, branch: UnitLoanForwardBranch) -> ForwardScope {
    let UnitLoanTarget::Place(place) = fact.source_loan().target() else {
        panic!("source")
    };
    ForwardScope {
        region: None,
        callable: branch.callable(),
        body: branch.value(),
        control: branch.control(),
        branch: Some(branch),
        branch_entry: None,
        argument: fact.source_loan().argument(),
        root: place.root(),
    }
}

fn recorded(pairs: &[(UnitBorrowForwardTransportFact, UnitLoanForwardBranch)]) -> ActionSource {
    recorded_with_join(pairs, false)
}

fn recorded_with_join(
    pairs: &[(UnitBorrowForwardTransportFact, UnitLoanForwardBranch)],
    extra_continuation: bool,
) -> ActionSource {
    let expected = [
        scope(&pairs[0].0, pairs[0].1),
        scope(&pairs[1].0, pairs[1].1),
    ];
    let mut actions = ActionSource::default();
    let id = actions.begin_branch_region(expected, (0, 0), true);
    actions.branch_condition(id, false, true);
    actions.branch_condition(id, true, true);
    for (i, (fact, _)) in pairs.iter().enumerate() {
        let mut actual = expected[i];
        actual.branch_entry = actions.enter_forward_branch(actual.branch.unwrap());
        actions.branch_arm(id, actual, true);
        if i == 1 {
            if extra_continuation {
                actions.branch_continuation(id, true, &[Some(actual), Some(actual)], true);
            } else {
                actions.branch_continuation(id, true, &[Some(actual)], true);
            }
        }
        let acquisition = actions.begin(fact.source_loan().clone()).unwrap();
        actions.branch_return(id, actual, false, true);
        actions.forward_handoff(
            fact.clone(),
            Some(ForwardCause {
                acquisition,
                scope: actual,
            }),
            Some(actual),
        );
        actions.branch_return(id, actual, true, true);
    }
    actions.seal_branch_region(id, true, true);
    actions.close_branch_callable(expected[0].callable);
    actions
}

fn partial(actions: ActionSource, source: SourceUnitId, file: SourceId, preserve_causes: bool) {
    let observations = actions.finish(source, file);
    let [catalog] = observations.as_slice() else {
        panic!("catalog")
    };
    assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
    assert!(catalog.forward_regions().is_empty());
    assert!(!catalog.branch_forward_regions().is_empty());
    for region in catalog.branch_forward_regions() {
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            region.paths().is_empty(),
            "Partial never leaks certified action handles"
        );
    }
    if preserve_causes {
        let handoffs = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), 2);
        assert!(
            handoffs.iter().all(|h| h.source_begin().is_some()
                && h.callable().is_some()
                && h.branch().is_some())
        );
    }
}

#[test]
fn ci_only_branch_certificate_requires_all_control_and_interval_witnesses() {
    let fixture = Fixture::new(&[("coverage.ko", SOURCE)]);
    let owned = fixture.shared();
    let mut pairs = owned.loan_action_observations()[0]
        .actions()
        .iter()
        .filter_map(|action| match action {
            UnitLoanAction::BorrowForwardHandoff(handoff) => {
                Some((handoff.transaction().clone(), handoff.branch().unwrap()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    pairs.sort_by_key(|(_, branch)| !branch.then_arm());
    assert_eq!(pairs.len(), 2);
    let source = pairs[0].0.returned_expression().source_unit();
    let file = fixture.parsed[0].source_id();
    let valid = recorded(&pairs).finish(source, file);
    let region = &valid[0].branch_forward_regions()[0];
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    assert_eq!(region.paths().len(), 2);
    assert!(valid[0].owns_branch_forward_region(region));
    let other = recorded(&pairs).finish(source, file);
    assert!(!other[0].owns_branch_forward_region(region));
    assert!(other[0].action(&region.paths()[0].source_begin()).is_none());

    for fault in 0..24 {
        let mut actions = recorded_with_join(&pairs, fault == 18);
        match fault {
            0 => actions.branch_forward_regions[0].arms[0] = None,
            1 => actions.branch_forward_regions[0].arms[1] = None,
            2 => actions.branch_forward_regions[0].condition = None,
            3 => actions.branch_forward_regions[0].split = None,
            4 => actions.branch_forward_regions[0].continuation = false,
            5 => {
                actions.branch_forward_regions[0].arms[0]
                    .as_mut()
                    .unwrap()
                    .pre = None
            }
            6 => {
                actions.branch_forward_regions[0].arms[1]
                    .as_mut()
                    .unwrap()
                    .end = None
            }
            7 => actions.branch_forward_regions[0].end = None,
            8 => actions.branch_forward_regions[0].closed = None,
            9 => actions.branch_forward_regions[0].invalid = true,
            10 => {
                actions.branch_forward_regions[0].arms[1]
                    .as_mut()
                    .unwrap()
                    .start = 0
            }
            11 => actions.branch_forward_regions[0].start = 1,
            12 => {
                let expected = actions.branch_forward_regions[0].expected;
                actions.begin_branch_region(expected, (0, 0), true);
            }
            13 => {
                let left = actions.branch_forward_regions[0].expected[0];
                actions.branch_forward_regions[0].expected[1] = left;
            }
            14 => {
                actions.branch_forward_regions[0].expected[1]
                    .branch
                    .as_mut()
                    .unwrap()
                    .condition = pairs[0].0.source_loan().argument()
            }
            15 => {
                actions.branch_forward_regions[0].expected[1].callable =
                    UnitItemId::new(source, fixture.parsed[0].roots()[0])
            }
            16 => {
                let callable = actions.branch_forward_regions[0].expected[0].callable;
                actions.close_branch_callable(callable);
            }
            17 => actions.seal_branch_region(0, true, true),
            // Injected at the original join, before seal/close can mask its rejection.
            18 => {}
            19 => {
                let actual = actions.branch_forward_regions[0].arms[0]
                    .as_ref()
                    .unwrap()
                    .scope;
                actions.branch_return(0, actual, false, true);
            }
            20 => {
                actions
                    .records
                    .insert(1, Record::Begin(pairs[0].0.source_loan().clone()));
            }
            21 => {
                actions.begin(pairs[0].0.source_loan().clone()).unwrap();
                actions.branch_forward_regions[0].closed = None;
                let callable = actions.branch_forward_regions[0].expected[0].callable;
                actions.close_branch_callable(callable);
            }
            22 => {
                let Record::BorrowForwardHandoff {
                    cause: Some(cause), ..
                } = &mut actions.records[1]
                else {
                    unreachable!()
                };
                cause.acquisition = ActionSource::default()
                    .begin(pairs[0].0.source_loan().clone())
                    .unwrap();
            }
            _ => {
                actions.branch_entries.clear();
            }
        }
        partial(actions, source, file, fault < 20 || fault == 21);
    }
}
