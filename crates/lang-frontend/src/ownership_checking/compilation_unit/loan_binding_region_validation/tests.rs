//! Constructed invalid coverage evidence is CI-only; do not execute locally.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction, compilation_unit::formed_tests::Fixture,
};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }";

fn recorded() -> (Candidate, Arc<()>, Vec<UnitLoanAction>) {
    let fixture = Fixture::new(&[("binding-coverage.ko", SOURCE)]);
    let shared = fixture.shared();
    let observation = &shared.loan_action_observations()[0];
    let region = &observation.binding_regions()[0];
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    let ids = region.action_ids().collect::<Vec<_>>();
    let UnitLoanAction::Begin(source) = observation.action(&ids[0]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Begin(consumer) = observation.action(&ids[2]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Cleanup {
        point: UnitDropPoint::AfterStatement(statement),
        action: UnitIterationCleanupAction::Drop(drop),
    } = observation.action(&ids[6]).unwrap()
    else {
        unreachable!()
    };
    let owner = OwnerSnapshot {
        owner: None,
        symbol: region.root(),
        origin: drop.value_origin(),
        declaration: drop.value_origin(),
        scope_depth: 1,
    };
    let candidate = Candidate {
        scope: Scope {
            callable: region.callable(),
            binding: region.binding(),
            root: region.root(),
            source_call: region.source_call(),
            consumer_call: region.consumer_call(),
            entry: region.entry_statement(),
            completion: region.completion_statement(),
            consumer_statement: *statement,
        },
        expected: Some(Expected {
            source: source.clone(),
            consumer: consumer.clone(),
            owner: owner.clone(),
        }),
        start: ids[0].index,
        end: Some(ids[6].index + 1),
        exit_empty: true,
        drop_link: Some((ids[5].clone(), ids[6].clone())),
        ambiguous: false,
        removed_owner: Some(owner),
    };
    (
        candidate,
        observation.identity.clone(),
        observation.actions().to_vec(),
    )
}

#[test]
fn ci_only_binding_coverage_rejects_missing_foreign_duplicate_and_wrong_drop_edges() {
    let (original, identity, actions) = recorded();
    assert_eq!(
        original.coverage(&identity, &actions),
        UnitLoanActionCoverage::Complete
    );
    for fault in 0..9 {
        let mut candidate = original.clone();
        match fault {
            0 => candidate.drop_link = None,
            1 => candidate.drop_link.as_mut().unwrap().0.identity = Arc::new(()),
            2 => candidate.drop_link.as_mut().unwrap().1.identity = Arc::new(()),
            3 => candidate.drop_link.as_mut().unwrap().0.index = candidate.start + 4,
            4 => candidate.drop_link.as_mut().unwrap().1.index = candidate.start + 5,
            5 => candidate.ambiguous = true,
            6 => candidate.removed_owner = None,
            7 => candidate.removed_owner.as_mut().unwrap().scope_depth += 1,
            _ => candidate.removed_owner.as_mut().unwrap().symbol = candidate.scope.binding,
        }
        assert_eq!(
            candidate.coverage(&identity, &actions),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn ci_only_binding_coverage_rejects_incomplete_boundaries_and_unfiltered_extra_actions() {
    let (original, identity, original_actions) = recorded();
    for fault in 0..6 {
        let mut candidate = original.clone();
        let mut actions = original_actions.clone();
        match fault {
            0 => candidate.end = None,
            1 => candidate.exit_empty = false,
            2 => candidate.start += 1,
            3 => {
                let extra = actions[candidate.start + 2].clone();
                actions.insert(candidate.start + 2, extra);
                *candidate.end.as_mut().unwrap() += 1;
            }
            4 => {
                let UnitLoanAction::Cleanup { point, .. } = &mut actions[candidate.start + 6]
                else {
                    unreachable!()
                };
                *point = UnitDropPoint::AfterStatement(candidate.scope.completion);
            }
            _ => candidate.scope.root = candidate.scope.binding,
        }
        assert_eq!(
            candidate.coverage(&identity, &actions),
            UnitLoanActionCoverage::Partial
        );
    }
}
