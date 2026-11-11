//! Constructed invalid coverage evidence is CI-only; do not execute locally.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction, compilation_unit::formed_tests::Fixture,
};

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

fn recorded() -> (
    Candidate,
    Arc<()>,
    Vec<UnitLoanAction>,
    Vec<UnitLoanAuxiliaryRetirement>,
) {
    let fixture = Fixture::new(&[("binding-coverage.ko", SOURCE)]);
    let shared = fixture.shared();
    let observation = &shared.loan_action_observations()[0];
    let region = &observation.auxiliary_binding_regions()[0];
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    let ids = region.action_ids().collect::<Vec<_>>();
    let UnitLoanAction::Begin(source) = observation.action(&ids[0]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Begin(auxiliary) = observation.action(&ids[1]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Begin(consumer) = observation.action(&ids[3]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Cleanup {
        point: UnitDropPoint::AfterStatement(statement),
        action: UnitIterationCleanupAction::Drop(drop),
    } = observation.action(&ids[7]).unwrap()
    else {
        unreachable!()
    };
    let owner_observation = shared
        .owner_observations()
        .iter()
        .find(|owner| {
            owner.callable()
                == crate::ownership_checking::UnitOwnerCallable::Function(region.callable())
        })
        .unwrap();
    let [definition] = owner_observation.definitions() else {
        panic!("the original caller forms exactly one owner")
    };
    assert!(matches!(
        definition.origin(),
        crate::ownership_checking::UnitOwnerOrigin::String(_)
    ));
    let owner = OwnerSnapshot {
        owner: Some(definition.id()),
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
            auxiliary: auxiliary.clone(),
            consumer: consumer.clone(),
            owner: owner.clone(),
        }),
        start: ids[0].index,
        end: Some(ids[7].index + 1),
        exit_empty: true,
        drop_link: Some((ids[6].clone(), ids[7].clone())),
        ambiguous: false,
        removed_owner: Some(owner),
    };
    (
        candidate,
        observation.identity.clone(),
        observation.actions().to_vec(),
        observation.auxiliary_retirements().to_vec(),
    )
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_missing_foreign_duplicate_and_wrong_drop_edges() {
    let (original, identity, actions, auxiliaries) = recorded();
    assert_eq!(
        original.coverage(&identity, &actions, &auxiliaries),
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
            candidate.coverage(&identity, &actions, &auxiliaries),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_incomplete_boundaries_and_unfiltered_extra_actions() {
    let (original, identity, original_actions, auxiliaries) = recorded();
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
                let UnitLoanAction::Cleanup { point, .. } = &mut actions[candidate.start + 7]
                else {
                    unreachable!()
                };
                *point = UnitDropPoint::AfterStatement(candidate.scope.completion);
            }
            _ => candidate.scope.root = candidate.scope.binding,
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &auxiliaries),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_missing_foreign_duplicate_or_mismatched_relation() {
    let (candidate, identity, actions, original) = recorded();
    for fault in 0..6 {
        let mut relations = original.clone();
        match fault {
            0 => relations.clear(),
            1 => relations[0].identity = Arc::new(()),
            2 => relations.push(relations[0].clone()),
            3 => relations[0].source_begin += 1,
            4 => relations[0].auxiliary_begin += 1,
            _ => relations[0].handoff += 1,
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_removed_and_reordered_records() {
    let (candidate, identity, original, auxiliaries) = recorded();
    for position in 0..8 {
        let mut removed = original.clone();
        removed.remove(candidate.start + position);
        assert_eq!(
            candidate.coverage(&identity, &removed, &auxiliaries),
            UnitLoanActionCoverage::Partial
        );
    }
    let mut swapped = original.clone();
    swapped.swap(candidate.start, candidate.start + 1);
    assert_eq!(
        candidate.coverage(&identity, &swapped, &auxiliaries),
        UnitLoanActionCoverage::Partial
    );
    let mut missing = candidate;
    missing.expected = None;
    assert_eq!(
        missing.coverage(&identity, &original, &auxiliaries),
        UnitLoanActionCoverage::Unsupported
    );
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_absent_expected_owner_identity() {
    let (mut candidate, identity, actions, auxiliaries) = recorded();
    assert_eq!(
        candidate.coverage(&identity, &actions, &auxiliaries),
        UnitLoanActionCoverage::Complete
    );
    candidate.expected.as_mut().unwrap().owner.owner = None;
    // Matching missing snapshots must not stand in for an actual formed owner.
    candidate.removed_owner.as_mut().unwrap().owner = None;
    assert_eq!(
        candidate.coverage(&identity, &actions, &auxiliaries),
        UnitLoanActionCoverage::Partial
    );
}
