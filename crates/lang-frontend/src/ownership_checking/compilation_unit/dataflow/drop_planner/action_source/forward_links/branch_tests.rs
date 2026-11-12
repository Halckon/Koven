//! Injected branch-cause boundary tests: CI-only, compile but never execute locally.
use super::super::super::borrow_forward_transport::{ForwardCause, ForwardScope};
use super::super::ActionSource;
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{
    UnitBorrowForwardTransportFact, UnitLoanAction, UnitLoanActionCoverage, UnitLoanForwardBranch,
};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view(source) }; return view(source) }";

fn scope(fact: &UnitBorrowForwardTransportFact, branch: UnitLoanForwardBranch) -> ForwardScope {
    let UnitLoanTarget::Place(place) = fact.source_loan().target() else {
        panic!("root source")
    };
    ForwardScope {
        region: None,
        callable: branch.callable(),
        body: fact.returned_expression(),
        control: branch.control(),
        branch: Some(branch),
        branch_entry: None,
        argument: fact.source_loan().argument(),
        root: place.root(),
    }
}

fn append(
    actions: &mut ActionSource,
    fact: &UnitBorrowForwardTransportFact,
    mut scope: ForwardScope,
) {
    scope.branch_entry = actions.enter_forward_branch(scope.branch.unwrap());
    let acquisition = actions.begin(fact.source_loan().clone()).unwrap();
    actions.forward_handoff(
        fact.clone(),
        Some(ForwardCause { acquisition, scope }),
        Some(scope),
    );
}

fn assert_rejected(actions: ActionSource, source: SourceUnitId, file: SourceId, count: usize) {
    let observations = actions.finish(source, file);
    let [catalog] = observations.as_slice() else {
        panic!("one catalog")
    };
    assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
    assert!(catalog.forward_regions().is_empty());
    let handoffs = catalog
        .actions()
        .iter()
        .filter_map(|action| match action {
            UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        handoffs.len(),
        count,
        "raw handoffs are not erased by failed certification"
    );
    for handoff in handoffs {
        assert!(handoff.source_begin().is_none());
        assert!(handoff.callable().is_none());
        assert!(handoff.branch().is_none());
    }
}

#[test]
fn ci_only_branch_cause_identity_faults_never_publish_any_causal_getter() {
    let fixture = Fixture::new(&[("branch.ko", SOURCE)]);
    let owned = fixture.shared();
    let catalog = &owned.loan_action_observations()[0];
    let pairs = catalog
        .actions()
        .iter()
        .filter_map(|action| match action {
            UnitLoanAction::BorrowForwardHandoff(handoff) => Some((
                handoff.transaction().clone(),
                handoff.branch().expect("real branch entry"),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(pairs.len(), 2);
    assert_ne!(pairs[0].1.then_arm(), pairs[1].1.then_arm());
    let (fact, branch) = &pairs[0];
    let original = scope(fact, *branch);
    let source = fact.returned_expression().source_unit();
    let file = fixture.parsed[0].source_id();
    // Positive baseline: real source products can be linked without granting coverage.
    let mut valid = ActionSource::default();
    append(&mut valid, fact, original);
    let valid = valid.finish(source, file);
    assert_eq!(valid[0].coverage(), UnitLoanActionCoverage::Partial);
    assert!(valid[0].forward_regions().is_empty());
    let UnitLoanAction::BorrowForwardHandoff(handoff) = &valid[0].actions()[1] else {
        unreachable!()
    };
    assert_eq!(handoff.branch(), Some(*branch));
    assert_eq!(handoff.callable(), Some(original.callable));
    assert_eq!(
        valid[0].action(&handoff.source_begin().unwrap()),
        Some(&UnitLoanAction::Begin(fact.source_loan().clone()))
    );

    for fault in 0..10 {
        let mut actions = ActionSource::default();
        append(&mut actions, fact, original);
        if fault == 0 {
            actions.branch_entries.clear();
        } else if fault == 1 {
            // Different registry index and Begin, but the exact same branch: reject BOTH.
            append(&mut actions, fact, original);
        } else if fault == 2 {
            let acquisition = match &actions.records[1] {
                Record::BorrowForwardHandoff {
                    cause: Some(cause), ..
                } => cause.acquisition.clone(),
                _ => unreachable!(),
            };
            append(&mut actions, &pairs[1].0, scope(&pairs[1].0, pairs[1].1));
            let Record::BorrowForwardHandoff {
                cause: Some(second),
                ..
            } = &mut actions.records[3]
            else {
                unreachable!()
            };
            second.acquisition = acquisition;
        } else {
            let Record::BorrowForwardHandoff {
                cause, completed, ..
            } = &mut actions.records[1]
            else {
                unreachable!()
            };
            match fault {
                3 => {
                    cause.as_mut().unwrap().acquisition = ActionSource::default()
                        .begin(fact.source_loan().clone())
                        .unwrap()
                }
                4 => {
                    let cause = cause.as_mut().unwrap();
                    cause.scope.branch.as_mut().unwrap().then_arm = !branch.then_arm();
                    *completed = Some(cause.scope);
                }
                5 => {
                    let cause = cause.as_mut().unwrap();
                    cause.scope.control = cause.scope.body;
                    cause.scope.branch.as_mut().unwrap().control = cause.scope.body;
                    *completed = Some(cause.scope);
                }
                6 => {
                    let cause = cause.as_mut().unwrap();
                    cause.scope.callable = UnitItemId::new(source, fixture.parsed[0].roots()[0]);
                    cause.scope.branch.as_mut().unwrap().callable = cause.scope.callable;
                    *completed = Some(cause.scope);
                }
                7 => *cause = None,
                8 => *completed = None,
                _ => {
                    let cause = cause.as_mut().unwrap();
                    cause.scope.branch.as_mut().unwrap().condition = cause.scope.argument;
                    *completed = Some(cause.scope);
                }
            }
        }
        assert_rejected(
            actions,
            source,
            file,
            if fault == 1 || fault == 2 { 2 } else { 1 },
        );
    }
}
