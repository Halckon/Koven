//! Constructed malformed evidence is CI-only. Compile locally; never execute locally.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction, UnitOwnerCallable, UnitOwnerId,
    compilation_unit::formed_tests::Fixture,
};

const SOURCE: &str = r#"fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun run(): Unit { val owner = "kept"; borrow val parent = view(owner); borrow val child = parent; consume(child) }"#;

struct Recorded {
    candidate: Candidate,
    identity: Arc<()>,
    actions: Vec<UnitLoanAction>,
    dependency: UnitLoanBindingDependencyDrop,
    owner: OwnerSnapshot,
}

fn recorded() -> Recorded {
    let fixture = Fixture::new(&[("child-coverage.ko", SOURCE)]);
    let shared = fixture.shared();
    let observation = &shared.loan_action_observations()[0];
    let [region] = observation.child_regions() else {
        panic!("one independently complete real interval required")
    };
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    let ids = region.action_ids().collect::<Vec<_>>();
    assert_eq!(ids.len(), 10);
    let UnitLoanAction::Begin(source) = observation.action(&ids[0]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::BorrowBindingChildCommitted(commit) = observation.action(&ids[2]).unwrap()
    else {
        unreachable!()
    };
    let UnitLoanAction::Begin(consumer) = observation.action(&ids[3]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Cleanup {
        point: UnitDropPoint::AfterStatement(statement),
        action: UnitIterationCleanupAction::Drop(drop),
    } = observation.action(&ids[9]).unwrap()
    else {
        unreachable!()
    };
    // Whole-callable owner definitions may be withdrawn for opaque calls. This test-only
    // witness exercises equality, not production acquisition: production compares entry
    // state with the same actual AppendedDrop.removed_owner and exposes neither token.
    let owner = OwnerSnapshot {
        owner: Some(UnitOwnerId {
            identity: Arc::new(()),
            callable: UnitOwnerCallable::Function(region.callable()),
            index: 0,
        }),
        symbol: region.root(),
        origin: drop.value_origin(),
        declaration: drop.value_origin(),
        scope_depth: 1,
    };
    let candidate = Candidate {
        scope: Scope {
            callable: region.callable(),
            parent: region.parent(),
            child: region.child(),
            root: region.root(),
            child_initializer: commit.initializer(),
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
        end: Some(ids[9].index + 1),
        exit_empty: true,
        ambiguous: false,
    };
    let [dependency] = observation.binding_dependency_drops() else {
        panic!("one actual dependency-to-Drop relation required")
    };
    Recorded {
        candidate,
        identity: observation.identity.clone(),
        actions: observation.actions().to_vec(),
        dependency: dependency.clone(),
        owner,
    }
}

#[test]
fn ci_only_child_coverage_rejects_unfiltered_records_ends_and_open_boundaries() {
    let recorded = recorded();
    assert_eq!(
        recorded.candidate.coverage(
            &recorded.identity,
            &recorded.actions,
            Some(&recorded.dependency),
            Some(recorded.owner.clone()),
        ),
        UnitLoanActionCoverage::Complete,
    );
    for fault in 0..8 {
        let mut candidate = recorded.candidate.clone();
        let mut actions = recorded.actions.clone();
        match fault {
            0 => candidate.end = None,
            1 => candidate.exit_empty = false,
            2 => candidate.ambiguous = true,
            3 => candidate.start += 1,
            4 => {
                actions.remove(candidate.start + 5);
                *candidate.end.as_mut().unwrap() -= 1;
            }
            5 => {
                actions.insert(candidate.start + 5, actions[candidate.start + 5].clone());
                *candidate.end.as_mut().unwrap() += 1;
            }
            6 => {
                let UnitLoanAction::BorrowBindingEnd(end) = &mut actions[candidate.start + 7]
                else {
                    unreachable!()
                };
                end.point = UnitDropPoint::AfterStatement(candidate.scope.entry);
            }
            _ => {
                let UnitLoanAction::Cleanup { point, .. } = &mut actions[candidate.start + 9]
                else {
                    unreachable!()
                };
                *point = UnitDropPoint::AfterStatement(candidate.scope.completion);
            }
        }
        assert_eq!(
            candidate.coverage(
                &recorded.identity,
                &actions,
                Some(&recorded.dependency),
                Some(recorded.owner.clone()),
            ),
            UnitLoanActionCoverage::Partial,
        );
    }
}

#[test]
fn ci_only_child_coverage_rejects_missing_foreign_dependency_and_owner_tokens() {
    let recorded = recorded();
    for fault in 0..8 {
        let mut dependency = Some(recorded.dependency.clone());
        let mut owner = Some(recorded.owner.clone());
        match fault {
            0 => dependency = None,
            1 => dependency.as_mut().unwrap().identity = Arc::new(()),
            2 => dependency.as_mut().unwrap().cleanup -= 1,
            3 => owner = None,
            4 => owner.as_mut().unwrap().owner = None,
            5 => owner.as_mut().unwrap().owner.as_mut().unwrap().identity = Arc::new(()),
            6 => owner.as_mut().unwrap().owner.as_mut().unwrap().index += 1,
            _ => owner.as_mut().unwrap().scope_depth += 1,
        }
        assert_eq!(
            recorded.candidate.coverage(
                &recorded.identity,
                &recorded.actions,
                dependency.as_ref(),
                owner,
            ),
            UnitLoanActionCoverage::Partial,
        );
    }
    let mut unsupported = recorded.candidate;
    unsupported.expected = None;
    assert_eq!(
        unsupported.coverage(
            &recorded.identity,
            &recorded.actions,
            Some(&recorded.dependency),
            Some(recorded.owner),
        ),
        UnitLoanActionCoverage::Unsupported,
    );
}
