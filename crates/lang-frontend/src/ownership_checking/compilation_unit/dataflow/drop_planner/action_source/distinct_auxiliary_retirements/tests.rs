//! CI-only mutation of actual two-root pending/owner observations. Never execute locally.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use std::cell::RefCell;
thread_local! {static SNAPSHOTS:RefCell<Option<Vec<(ActionSource,SourceUnitId,SourceId)>>>=const{RefCell::new(None)};}
pub(super) fn record(source: &ActionSource, unit: SourceUnitId, file: SourceId) {
    SNAPSHOTS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(snapshots) = slot.as_mut() else {
            return;
        };
        let Some(last) = source
            .distinct_auxiliary_retirements
            .iter()
            .map(|c| c.handoff.index)
            .max()
        else {
            return;
        };
        let records = source
            .records
            .iter()
            .take(last + 1)
            .map(|r| match r {
                Record::Begin(loan) => Record::Begin(loan.clone()),
                Record::BorrowHandoffCommitted(fact) => {
                    Record::BorrowHandoffCommitted(fact.clone())
                }
                Record::Cleanup((point, action)) => Record::Cleanup((*point, action.clone())),
                _ => {
                    panic!("fixture uses only actual acquisition and normal-return prefix records")
                }
            })
            .collect();
        snapshots.push((
            ActionSource {
                identity: source.identity.clone(),
                records,
                distinct_auxiliary_retirements: source.distinct_auxiliary_retirements.clone(),
                ..ActionSource::default()
            },
            unit,
            file,
        ));
    });
}
fn capture(second: bool) -> (ActionSource, SourceUnitId, SourceId) {
    capture_drop(second, false)
}
fn capture_drop(second: bool, dropped: bool) -> (ActionSource, SourceUnitId, SourceId) {
    let (params, args) = if second {
        ("aux: String, source: String", "right, left")
    } else {
        ("source: String, aux: String", "left, right")
    };
    let text = format!(
        "fun view({params}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val left = \"left\"; val right = \"right\"; borrow val item = view({args}); consume(item); consume(right) }}"
    );
    let text = if dropped {
        text.replace("; consume(right)", "")
    } else {
        text
    };
    let fixture = Fixture::new(&[("distinct-ci.ko", &text)]);
    SNAPSHOTS.with(|s| *s.borrow_mut() = Some(Vec::new()));
    let _owned = fixture.shared();
    let mut snapshots = SNAPSHOTS.with(|s| s.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    let (source, unit, file) = snapshots.pop().unwrap();
    let result = source.validate_distinct_auxiliary_retirements(unit, file);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].source_parameter(), usize::from(second));
    assert_ne!(result[0].source_owner(), result[0].auxiliary_owner());
    assert_eq!(result[0].auxiliary_drop().is_some(), dropped);
    (source, unit, file)
}
#[test]
fn ci_only_distinct_retirement_rejects_token_owner_role_and_raw_corruption() {
    for second in [false, true] {
        for fault in 0..22 {
            let (mut source, unit, file) = capture(second);
            let (foreign, _, _) = capture(second);
            let c = &source.distinct_auxiliary_retirements[0];
            let source_handle = c.source.clone().unwrap();
            let auxiliary = c.auxiliaries[0].clone().unwrap();
            let handoff = c.handoff.clone();
            let witness = c.witness.clone();
            match fault {
                0 => source.distinct_auxiliary_retirements[0].source = None,
                1 => source.distinct_auxiliary_retirements[0].auxiliaries[0] = None,
                2 => source.distinct_auxiliary_retirements[0].source_live = false,
                3 => source.distinct_auxiliary_retirements[0].witness = None,
                4 => {
                    source.distinct_auxiliary_retirements[0].source =
                        foreign.distinct_auxiliary_retirements[0].source.clone()
                }
                5 => {
                    source.distinct_auxiliary_retirements[0].auxiliaries = foreign
                        .distinct_auxiliary_retirements[0]
                        .auxiliaries
                        .clone()
                }
                6 => {
                    source.distinct_auxiliary_retirements[0].handoff =
                        foreign.distinct_auxiliary_retirements[0].handoff.clone()
                }
                7 => {
                    source.distinct_auxiliary_retirements[0].auxiliaries =
                        vec![Some(source_handle.clone())]
                }
                8 => source.register_distinct_auxiliary_retirement(
                    handoff.clone(),
                    Some(source_handle.clone()),
                    vec![Some(auxiliary.clone())],
                    true,
                    witness,
                    None,
                ),
                9 => source.register_distinct_auxiliary_retirement(
                    foreign.distinct_auxiliary_retirements[0].handoff.clone(),
                    None,
                    vec![Some(source_handle.clone())],
                    false,
                    None,
                    None,
                ),
                10 => source.register_distinct_auxiliary_retirement(
                    foreign.distinct_auxiliary_retirements[0].handoff.clone(),
                    Some(auxiliary.clone()),
                    vec![],
                    false,
                    None,
                    None,
                ),
                11 => {
                    source.distinct_auxiliary_retirements[0]
                        .witness
                        .as_mut()
                        .unwrap()
                        .source_parameter = 1 - usize::from(second)
                }
                12 => {
                    source.distinct_auxiliary_retirements[0]
                        .witness
                        .as_mut()
                        .unwrap()
                        .source_owner
                        .owner = None
                }
                13 => {
                    let c = source.distinct_auxiliary_retirements[0]
                        .witness
                        .as_mut()
                        .unwrap();
                    c.auxiliary_owner.owner = c.source_owner.owner.clone();
                }
                14 => {
                    source.distinct_auxiliary_retirements[0]
                        .witness
                        .as_mut()
                        .unwrap()
                        .source_owner
                        .owner = foreign.distinct_auxiliary_retirements[0]
                        .witness
                        .as_ref()
                        .unwrap()
                        .source_owner
                        .owner
                        .clone()
                }
                15 => {
                    let w = source.distinct_auxiliary_retirements[0]
                        .witness
                        .as_mut()
                        .unwrap();
                    w.auxiliary_owner.symbol = w.source_owner.symbol;
                }
                16 => {
                    source.distinct_auxiliary_retirements[0]
                        .witness
                        .as_mut()
                        .unwrap()
                        .source_owner
                        .scope_depth = 2
                }
                17 => {
                    let Record::Begin(loan) = &source.records[source_handle.index] else {
                        panic!("actual begin")
                    };
                    source.records.push(Record::Begin(loan.clone()));
                }
                18 => {
                    let Record::BorrowHandoffCommitted(fact) = &source.records[handoff.index]
                    else {
                        panic!("actual handoff")
                    };
                    source
                        .records
                        .push(Record::BorrowHandoffCommitted(fact.clone()));
                }
                19 => {
                    let Record::Begin(loan) = &source.records[source_handle.index] else {
                        panic!("actual begin")
                    };
                    source.records[auxiliary.index] = Record::Begin(loan.clone());
                }
                20 => {
                    let Record::Begin(loan) = &source.records[source_handle.index] else {
                        panic!("actual begin")
                    };
                    source.records.insert(
                        0,
                        Record::Cleanup((
                            super::super::super::PlannerDropPoint::CallReturn(
                                loan.call().expression(),
                            ),
                            UnitIterationCleanupAction::EndCallLoan(loan.clone()),
                        )),
                    );
                    let c = &mut source.distinct_auxiliary_retirements[0];
                    c.source.as_mut().unwrap().index += 1;
                    c.auxiliaries[0].as_mut().unwrap().index += 1;
                    c.handoff.index += 1;
                }
                _ => {
                    source.distinct_auxiliary_retirements[0]
                        .witness
                        .as_mut()
                        .unwrap()
                        .source_parameter = 2
                }
            }
            assert!(
                source
                    .validate_distinct_auxiliary_retirements(unit, file)
                    .is_empty(),
                "second={second} fault={fault}"
            );
        }
    }
}
#[test]
fn ci_only_distinct_relation_does_not_claim_future_source_lifetime() {
    let (mut source, unit, file) = capture(false);
    let begin = source.distinct_auxiliary_retirements[0]
        .source
        .as_ref()
        .unwrap()
        .index;
    let Record::Begin(loan) = &source.records[begin] else {
        panic!("actual begin")
    };
    source.records.push(Record::Cleanup((
        super::super::super::PlannerDropPoint::CallReturn(loan.call().expression()),
        UnitIterationCleanupAction::EndCallLoan(loan.clone()),
    )));
    assert_eq!(
        source
            .validate_distinct_auxiliary_retirements(unit, file)
            .len(),
        1
    );
}

#[test]
fn ci_only_distinct_asap_drop_requires_exact_real_owner_and_cleanup() {
    for second in [false, true] {
        for fault in 0..7 {
            let (mut source, unit, file) = capture_drop(second, true);
            let c = &source.distinct_auxiliary_retirements[0];
            let drop_index = c.handoff.index - 1;
            let witness = c.witness.clone().unwrap();
            match fault {
                0 => source.distinct_auxiliary_retirements[0].owner_drop = None,
                1 => {
                    let Some(UnitOwnerTransportEvent::Drop { owner, .. }) =
                        source.distinct_auxiliary_retirements[0].owner_drop.as_mut()
                    else {
                        panic!("actual owner drop")
                    };
                    *owner = witness.source_owner.owner.clone().unwrap();
                }
                2 => {
                    let Some(UnitOwnerTransportEvent::Drop { symbol, .. }) =
                        source.distinct_auxiliary_retirements[0].owner_drop.as_mut()
                    else {
                        panic!("actual owner drop")
                    };
                    *symbol = witness.source_owner.symbol;
                }
                3 => {
                    let Record::Cleanup((point, _)) = &mut source.records[drop_index] else {
                        panic!("actual cleanup")
                    };
                    *point = super::super::super::PlannerDropPoint::AfterExpression(
                        witness.call.expression(),
                    );
                }
                4 => {
                    let Record::Cleanup((_, UnitIterationCleanupAction::Drop(drop))) =
                        &mut source.records[drop_index]
                    else {
                        panic!("actual drop")
                    };
                    *drop = crate::ownership_checking::UnitDropFact::new(
                        drop.point(),
                        UnitDropTarget::Named(witness.source_owner.symbol),
                        drop.value_origin(),
                    );
                }
                5 => {
                    let Record::Cleanup((point, action)) = &source.records[drop_index] else {
                        panic!("actual cleanup")
                    };
                    source
                        .records
                        .insert(drop_index, Record::Cleanup((*point, action.clone())));
                    source.distinct_auxiliary_retirements[0].handoff.index += 1;
                }
                _ => {
                    let (foreign, _, _) = capture_drop(second, true);
                    source.distinct_auxiliary_retirements[0].owner_drop =
                        foreign.distinct_auxiliary_retirements[0].owner_drop.clone();
                }
            }
            assert!(
                source
                    .validate_distinct_auxiliary_retirements(unit, file)
                    .is_empty(),
                "ASAP fault {fault}"
            );
        }
    }
}

#[test]
fn ci_only_distinct_owner_event_suffix_is_exact_at_publication() {
    for second in [false, true] {
        for dropped in [false, true] {
            for fault in 0..4 {
                let (mut source, unit, file) = capture_drop(second, dropped);
                let candidate = &mut source.distinct_auxiliary_retirements[0];
                let witness = candidate.witness.as_mut().unwrap();
                let source_drop = UnitOwnerTransportEvent::Drop {
                    point: UnitDropPoint::CallReturn(witness.call),
                    symbol: witness.source_owner.symbol,
                    owner: witness.source_owner.owner.clone().unwrap(),
                };
                match fault {
                    0 => witness.owner_events.push(source_drop),
                    1 => witness.owner_events.push(UnitOwnerTransportEvent::Move {
                        expression: witness.call,
                        symbol: witness.source_owner.symbol,
                        owner: witness.source_owner.owner.clone().unwrap(),
                    }),
                    2 if dropped => witness.owner_events.clear(),
                    2 => witness.owner_events.push(UnitOwnerTransportEvent::Drop {
                        point: UnitDropPoint::CallReturn(witness.call),
                        symbol: witness.auxiliary_owner.symbol,
                        owner: witness.auxiliary_owner.owner.clone().unwrap(),
                    }),
                    _ => witness.owner_events = vec![source_drop],
                }
                assert!(
                    source
                        .validate_distinct_auxiliary_retirements(unit, file)
                        .is_empty(),
                    "second={second} dropped={dropped} suffix fault={fault}"
                );
            }
        }
    }
}
