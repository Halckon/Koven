//! Constructed invalid coverage evidence is CI-only; do not execute locally.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction, compilation_unit::formed_tests::Fixture,
};

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; val other = \"aux\"; borrow val item = view(owner, other); consume(item) }";

fn recorded_with(
    text: &str,
    source_index: usize,
) -> (
    Candidate,
    Arc<()>,
    Vec<UnitLoanAction>,
    Vec<UnitLoanDistinctAuxiliaryRetirement>,
) {
    let fixture = Fixture::new(&[("binding-coverage.ko", text)]);
    let shared = fixture.shared();
    let observation = &shared.loan_action_observations()[0];
    let region = &observation.distinct_auxiliary_binding_regions()[0];
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    let ids = region.action_ids().collect::<Vec<_>>();
    let UnitLoanAction::Begin(source) = observation.action(&ids[source_index]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Begin(auxiliary) = observation.action(&ids[1 - source_index]).unwrap()
    else {
        unreachable!()
    };
    let UnitLoanAction::Begin(consumer) = observation.action(&ids[4]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Cleanup {
        point: UnitDropPoint::AfterStatement(statement),
        action: UnitIterationCleanupAction::Drop(drop),
    } = observation.action(&ids[8]).unwrap()
    else {
        unreachable!()
    };
    let UnitLoanAction::Cleanup {
        action: UnitIterationCleanupAction::Drop(aux_drop),
        ..
    } = observation.action(&ids[2]).unwrap()
    else {
        unreachable!()
    };
    let owner = OwnerSnapshot {
        owner: region.source_owner().cloned(),
        symbol: region.root(),
        origin: drop.value_origin(),
        declaration: drop.value_origin(),
        scope_depth: 1,
    };
    let auxiliary_owner = OwnerSnapshot {
        owner: region.auxiliary_owner().cloned(),
        symbol: region.auxiliary_root().unwrap(),
        origin: aux_drop.value_origin(),
        declaration: aux_drop.value_origin(),
        scope_depth: 1,
    };
    let owner_events = vec![
        super::super::UnitOwnerTransportEvent::Drop {
            point: aux_drop.point(),
            symbol: auxiliary_owner.symbol,
            owner: auxiliary_owner.owner.clone().unwrap(),
        },
        super::super::UnitOwnerTransportEvent::Drop {
            point: drop.point(),
            symbol: owner.symbol,
            owner: owner.owner.clone().unwrap(),
        },
    ];
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
            source_index,
            source: source.clone(),
            auxiliary: auxiliary.clone(),
            consumer: consumer.clone(),
            source_owner: owner.clone(),
            auxiliary_owner,
        }),
        start: ids[0].index,
        end: Some(ids[8].index + 1),
        exit_empty: true,
        drop_link: Some((ids[7].clone(), ids[8].clone())),
        ambiguous: false,
        removed_source_owner: Some(owner),
        owner_events,
    };
    (
        candidate,
        observation.identity.clone(),
        observation.actions().to_vec(),
        observation.distinct_auxiliary_retirements().to_vec(),
    )
}

fn recorded() -> (
    Candidate,
    Arc<()>,
    Vec<UnitLoanAction>,
    Vec<UnitLoanDistinctAuxiliaryRetirement>,
) {
    recorded_with(SOURCE, 0)
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
            6 => candidate.removed_source_owner = None,
            7 => candidate.removed_source_owner.as_mut().unwrap().scope_depth += 1,
            _ => candidate.removed_source_owner.as_mut().unwrap().symbol = candidate.scope.binding,
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
                let UnitLoanAction::Cleanup { point, .. } = &mut actions[candidate.start + 8]
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
    for position in 0..9 {
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
    candidate.expected.as_mut().unwrap().source_owner.owner = None;
    // Matching missing snapshots must not stand in for an actual formed owner.
    candidate.removed_source_owner.as_mut().unwrap().owner = None;
    assert_eq!(
        candidate.coverage(&identity, &actions, &auxiliaries),
        UnitLoanActionCoverage::Partial
    );
}

#[test]
fn ci_only_source_second_rejects_wrong_chronology_and_source_edges() {
    let text = SOURCE
        .replace(
            "view(source: String, aux: String)",
            "view(aux: String, source: String)",
        )
        .replace("view(owner, other)", "view(other, owner)");
    let (candidate, identity, actions, relations) = recorded_with(&text, 1);
    assert_eq!(
        candidate.coverage(&identity, &actions, &relations),
        UnitLoanActionCoverage::Complete
    );
    for fault in 0..6 {
        let mut candidate = candidate.clone();
        let mut actions = actions.clone();
        let mut relations = relations.clone();
        match fault {
            0 => candidate.expected.as_mut().unwrap().source_index = 0,
            1 => candidate.expected.as_mut().unwrap().source_index = 2,
            2 => actions.swap(candidate.start, candidate.start + 1),
            3 => relations[0].source_begin = candidate.start,
            4 => relations[0].auxiliary_begin = candidate.start + 1,
            _ => {
                let expected = candidate.expected.as_mut().unwrap();
                std::mem::swap(&mut expected.source, &mut expected.auxiliary);
            }
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Partial,
            "chronology fault {fault}"
        );
    }
}

#[test]
fn ci_only_distinct_region_rejects_owner_events_and_auxiliary_drop_corruption() {
    let (original, identity, actions, relations) = recorded();
    for fault in 0..7 {
        let mut candidate = original.clone();
        let mut relations = relations.clone();
        match fault {
            0 => candidate.owner_events.clear(),
            1 => candidate.owner_events.reverse(),
            2 => candidate
                .owner_events
                .push(candidate.owner_events[0].clone()),
            3 => candidate.expected.as_mut().unwrap().auxiliary_owner.owner = None,
            4 => {
                candidate.expected.as_mut().unwrap().auxiliary_owner.owner = candidate
                    .expected
                    .as_ref()
                    .unwrap()
                    .source_owner
                    .owner
                    .clone()
            }
            5 => relations[0].auxiliary_drop = None,
            _ => relations[0].auxiliary_drop = Some(candidate.start + 8),
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Partial,
            "owner fault {fault}"
        );
    }
}
