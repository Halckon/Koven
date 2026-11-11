//! Constructed invalid coverage evidence is CI-only; do not execute locally.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction, compilation_unit::formed_tests::Fixture,
};

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; val other = \"aux\"; borrow val item = view(owner, other); consume(item); consume(other) }";

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
    let region = &observation.later_auxiliary_binding_regions()[0];
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    let ids = region.action_ids().collect::<Vec<_>>();
    let UnitLoanAction::Begin(source) = observation.action(&ids[source_index]).unwrap() else {
        unreachable!()
    };
    let UnitLoanAction::Begin(auxiliary) = observation.action(&ids[1 - source_index]).unwrap()
    else {
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
    let UnitLoanAction::Cleanup {
        action: UnitIterationCleanupAction::Drop(aux_drop),
        ..
    } = observation.action(&ids[10]).unwrap()
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
            point: drop.point(),
            symbol: owner.symbol,
            owner: owner.owner.clone().unwrap(),
        },
        super::super::UnitOwnerTransportEvent::Drop {
            point: aux_drop.point(),
            symbol: auxiliary_owner.symbol,
            owner: auxiliary_owner.owner.clone().unwrap(),
        },
    ];
    let UnitLoanAction::Begin(later_consumer) = observation.action(&ids[8]).unwrap() else {
        unreachable!()
    };
    let mut candidate = Candidate {
        protocol: Protocol::Single,
        actual_returns: Vec::new(),
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
            auxiliary_prefix_count: 0,
            later_uses: vec![(later_consumer.clone(), region.later_statement().unwrap())],
            later_consumer: later_consumer.clone(),
            later_statement: region.later_statement().unwrap(),
            source_index,
            source: source.clone(),
            auxiliary: auxiliary.clone(),
            consumer: consumer.clone(),
            source_owner: owner.clone(),
            auxiliary_owner: auxiliary_owner.clone(),
        }),
        start: ids[0].index,
        end: Some(ids[10].index + 1),
        exit_empty: true,
        drop_link: Some((ids[6].clone(), ids[7].clone())),
        ambiguous: false,
        removed_source_owner: Some(owner),
        owner_events,
        later_return: Some(LaterReturn {
            source_obligation: None,
            loan: later_consumer.clone(),
            begin: region.later_begin().unwrap(),
            cursor: region.later_return_cursor().unwrap(),
            owner: auxiliary_owner.clone(),
        }),
        removed_auxiliary_owner: Some(auxiliary_owner),
        auxiliary_drop_link: Some((ids[8].clone(), ids[10].clone())),
        return_drop_claimed: true,
    };
    candidate.actual_returns = candidate.later_return.clone().into_iter().collect();
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
    for position in 0..11 {
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
            5 => relations[0].auxiliary_drop = Some(candidate.start + 2),
            _ => relations[0].auxiliary_drop = Some(candidate.start + 8),
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Partial,
            "owner fault {fault}"
        );
    }
}

#[test]
fn ci_only_later_return_and_auxiliary_destruction_require_actual_links() {
    let (original, identity, actions, relations) = recorded();
    for fault in 0..14 {
        let mut candidate = original.clone();
        match fault {
            0 => candidate.later_return = None,
            1 => candidate.later_return.as_mut().unwrap().begin.identity = Arc::new(()),
            2 => candidate.later_return.as_mut().unwrap().begin.index += 1,
            3 => candidate.later_return.as_mut().unwrap().cursor += 1,
            4 => candidate.later_return.as_mut().unwrap().owner.owner = None,
            5 => candidate.return_drop_claimed = false,
            6 => candidate.auxiliary_drop_link = None,
            7 => candidate.auxiliary_drop_link.as_mut().unwrap().0.identity = Arc::new(()),
            8 => candidate.auxiliary_drop_link.as_mut().unwrap().1.index -= 1,
            9 => candidate.removed_auxiliary_owner = None,
            10 => {
                candidate
                    .removed_auxiliary_owner
                    .as_mut()
                    .unwrap()
                    .scope_depth += 1
            }
            11 => {
                candidate.later_return.as_mut().unwrap().loan =
                    candidate.expected.as_ref().unwrap().consumer.clone()
            }
            12 => {
                candidate.expected.as_mut().unwrap().later_statement =
                    candidate.scope.consumer_statement
            }
            _ => {
                candidate.later_return.as_mut().unwrap().owner =
                    candidate.expected.as_ref().unwrap().source_owner.clone()
            }
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Partial,
            "later fault {fault}"
        );
    }
}

type SequenceCapture = (
    Candidate,
    Arc<()>,
    Vec<UnitLoanAction>,
    Vec<UnitLoanDistinctAuxiliaryRetirement>,
);
thread_local! {
    static SEQUENCE_CAPTURE: std::cell::RefCell<Option<Option<SequenceCapture>>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn record_sequence(
    candidate: &Candidate,
    identity: &Arc<()>,
    actions: &[UnitLoanAction],
    relations: &[UnitLoanDistinctAuxiliaryRetirement],
) {
    SEQUENCE_CAPTURE.with(|slot| {
        if let Some(captured) = slot.borrow_mut().as_mut() {
            assert!(captured.is_none(), "one actual sequence region");
            *captured = Some((
                candidate.clone(),
                identity.clone(),
                actions.to_vec(),
                relations.to_vec(),
            ));
        }
    });
}
fn recorded_sequence(count: usize, second: bool) -> SequenceCapture {
    let uses = "consume(other); ".repeat(count);
    let mut text = SOURCE.replace("consume(other) }", &format!("{uses}}}"));
    if second {
        text = text
            .replace(
                "view(source: String, aux: String)",
                "view(aux: String, source: String)",
            )
            .replace("view(owner, other)", "view(other, owner)");
    }
    SEQUENCE_CAPTURE.with(|slot| *slot.borrow_mut() = Some(None));
    let fixture = Fixture::new(&[("sequence-coverage.ko", &text)]);
    let shared = fixture.shared();
    assert_eq!(
        shared.loan_action_observations()[0].auxiliary_sequence_regions()[0].coverage(),
        UnitLoanActionCoverage::Complete
    );
    let captured = SEQUENCE_CAPTURE
        .with(|slot| slot.borrow_mut().take().flatten())
        .expect("actual candidate finish");
    assert_eq!(captured.0.actual_returns.len(), count);
    captured
}
#[test]
fn ci_only_sequence_requires_all_ordered_returns_and_last_destruction() {
    for count in [2, MAX_AUXILIARY_SEQUENCE_USES] {
        for second in [false, true] {
            let (original, identity, actions, relations) = recorded_sequence(count, second);
            assert_eq!(
                original.coverage_for(Protocol::Sequence, &identity, &actions, &relations),
                UnitLoanActionCoverage::Complete
            );
            assert_eq!(
                original.coverage(&identity, &actions, &relations),
                UnitLoanActionCoverage::Unsupported
            );
            for fault in 0..16 {
                let mut candidate = original.clone();
                match fault {
                    0 => candidate.protocol = Protocol::Single,
                    1 => candidate.expected.as_mut().unwrap().later_uses.truncate(1),
                    2 => {
                        let e = candidate.expected.as_mut().unwrap();
                        e.later_uses
                            .resize(MAX_AUXILIARY_SEQUENCE_USES + 1, e.later_uses[0].clone());
                    }
                    3 => {
                        candidate.actual_returns.remove(0);
                    }
                    4 => candidate.actual_returns.swap(0, 1),
                    5 => candidate.actual_returns[1] = candidate.actual_returns[0].clone(),
                    6 => candidate.actual_returns[0].begin.identity = Arc::new(()),
                    7 => candidate.actual_returns[0].cursor += 1,
                    8 => {
                        candidate.actual_returns[0].owner =
                            candidate.expected.as_ref().unwrap().source_owner.clone()
                    }
                    9 => candidate.return_drop_claimed = false,
                    10 => {
                        candidate.auxiliary_drop_link.as_mut().unwrap().1.index =
                            candidate.start + 10
                    }
                    11 => candidate.later_return = Some(candidate.actual_returns[0].clone()),
                    12 => candidate
                        .owner_events
                        .push(candidate.owner_events[1].clone()),
                    13 => candidate.expected.as_mut().unwrap().later_uses.swap(0, 1),
                    14 => candidate.removed_auxiliary_owner = None,
                    _ => {
                        candidate.actual_returns[0].loan =
                            candidate.expected.as_ref().unwrap().consumer.clone()
                    }
                }
                assert_ne!(
                    candidate.coverage_for(Protocol::Sequence, &identity, &actions, &relations),
                    UnitLoanActionCoverage::Complete,
                    "count={count} second={second} fault={fault}"
                );
            }
            for offset in 8..8 + 2 * count {
                let mut altered = actions.clone();
                altered.remove(original.start + offset);
                assert_eq!(
                    original.coverage_for(Protocol::Sequence, &identity, &altered, &relations),
                    UnitLoanActionCoverage::Partial
                );
            }
        }
    }
}
#[test]
fn ci_only_single_cannot_be_republished_as_sequence() {
    let (candidate, identity, actions, relations) = recorded();
    assert_eq!(
        candidate
            .clone()
            .finish_sequence(identity.clone(), &actions, &relations)
            .coverage(),
        UnitLoanActionCoverage::Unsupported
    );
    let mut forged = candidate;
    forged.protocol = Protocol::Sequence;
    assert_ne!(
        forged
            .finish_sequence(identity, &actions, &relations)
            .coverage(),
        UnitLoanActionCoverage::Complete
    );
}

fn recorded_interleaved(count: usize, prefix: usize) -> SequenceCapture {
    let before = "consume(other);".repeat(prefix);
    let after = "consume(other);".repeat(count - prefix);
    let text = SOURCE.replace(
        "consume(item); consume(other)",
        &format!("{before} consume(item); {after}"),
    );
    SEQUENCE_CAPTURE.with(|slot| *slot.borrow_mut() = Some(None));
    let fixture = Fixture::new(&[("interleaved-coverage.ko", &text)]);
    let shared = fixture.shared();
    assert_eq!(
        shared.loan_action_observations()[0].interleaved_auxiliary_regions()[0].coverage(),
        UnitLoanActionCoverage::Complete
    );
    SEQUENCE_CAPTURE
        .with(|slot| slot.borrow_mut().take().flatten())
        .expect("actual interleaved finish")
}
#[test]
fn ci_only_interleaved_requires_actual_source_phase_and_original_transport() {
    for (count, prefix) in [(1, 1), (2, 1), (2, 2), (8, 4), (8, 8)] {
        let (original, identity, actions, relations) = recorded_interleaved(count, prefix);
        assert_eq!(
            original.coverage_for(Protocol::Interleaved, &identity, &actions, &relations),
            UnitLoanActionCoverage::Complete
        );
        assert_ne!(
            original.coverage_for(Protocol::Sequence, &identity, &actions, &relations),
            UnitLoanActionCoverage::Complete
        );
        for fault in 0..18 {
            let mut c = original.clone();
            match fault {
                0 => c.protocol = Protocol::Single,
                1 => c.protocol = Protocol::Sequence,
                2 => c.expected.as_mut().unwrap().auxiliary_prefix_count = 0,
                3 => c.expected.as_mut().unwrap().auxiliary_prefix_count = count + 1,
                4 => c.actual_returns[0].source_obligation = None,
                5 => {
                    if let Some(SourceObligation::Before { owner, .. }) =
                        &mut c.actual_returns[0].source_obligation
                    {
                        owner.owner = None;
                    }
                }
                6 => {
                    if let Some(SourceObligation::Before { source_begin, .. }) =
                        &mut c.actual_returns[0].source_obligation
                    {
                        source_begin.identity = Arc::new(());
                    }
                }
                7 => {
                    if let Some(SourceObligation::Before { handoff, .. }) =
                        &mut c.actual_returns[0].source_obligation
                    {
                        handoff.index += 1;
                    }
                }
                8 => {
                    let (retirement, drop) = c.drop_link.clone().unwrap();
                    c.actual_returns[0].source_obligation =
                        Some(SourceObligation::After { retirement, drop });
                }
                9 => c.owner_events.swap(0, 1),
                10 => c.removed_source_owner = None,
                11 => c.removed_auxiliary_owner = None,
                12 => c.auxiliary_drop_link.as_mut().unwrap().1.index += 1,
                13 => c.drop_link.as_mut().unwrap().0.index += 1,
                14 => c.return_drop_claimed = false,
                15 => c.actual_returns[0].cursor += 1,
                16 => c.actual_returns[0].owner = c.expected.as_ref().unwrap().source_owner.clone(),
                _ => {
                    c.actual_returns.remove(0);
                }
            }
            assert_ne!(
                c.coverage_for(Protocol::Interleaved, &identity, &actions, &relations),
                UnitLoanActionCoverage::Complete,
                "n={count} p={prefix} fault={fault}"
            );
        }
        for index in original.start..original.end.unwrap() {
            let mut corrupted = actions.clone();
            corrupted.remove(index);
            assert_ne!(
                original.coverage_for(Protocol::Interleaved, &identity, &corrupted, &relations),
                UnitLoanActionCoverage::Complete
            );
        }
        if prefix < count {
            let mut c = original.clone();
            c.actual_returns[prefix].source_obligation =
                c.actual_returns[0].source_obligation.clone();
            assert_ne!(
                c.coverage_for(Protocol::Interleaved, &identity, &actions, &relations),
                UnitLoanActionCoverage::Complete
            );
        }
    }
}
