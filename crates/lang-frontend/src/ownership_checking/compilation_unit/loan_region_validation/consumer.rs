//! Closed result-chain validation over the complete, unfiltered normal-return interval.
use super::*;
use crate::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget,
};

pub(super) fn coverage(
    candidate: &Candidate,
    identity: &Arc<()>,
    all: &[UnitLoanAction],
) -> UnitLoanActionCoverage {
    let Some(expected) = &candidate.consumer else {
        return UnitLoanActionCoverage::Unsupported;
    };
    let Some(actions) = candidate.end.and_then(|end| all.get(candidate.start..end)) else {
        return UnitLoanActionCoverage::Partial;
    };
    let [
        UnitLoanAction::Begin(source),
        UnitLoanAction::BorrowConsumerTransfer {
            source: moved,
            consumer: call,
            argument,
        },
        UnitLoanAction::Begin(consumer),
        UnitLoanAction::BorrowConsumerRetired(retired),
        UnitLoanAction::Cleanup {
            point,
            action: UnitIterationCleanupAction::Drop(drop),
        },
    ] = actions
    else {
        return UnitLoanActionCoverage::Partial;
    };
    let call_return = UnitDropPoint::CallReturn(candidate.call);
    let file = expected.formation_span.source_id();
    let id_at = |id: Option<super::super::UnitLoanActionId>, offset| {
        id.is_some_and(|id| {
            Arc::ptr_eq(&id.identity, identity)
                && Some(id.index) == candidate.start.checked_add(offset)
        })
    };
    let valid = candidate
        .callable
        .is_some_and(|item| item.source_unit() == candidate.call.source_unit())
        && expected.root.source_unit() == candidate.call.source_unit()
        && source == &expected.source
        && moved == source
        && consumer == &expected.consumer
        && *call == candidate.call
        && *argument == source.call()
        && consumer.call() == candidate.call
        && consumer.argument() == source.call()
        && source.call() != candidate.call
        && source.argument().source_unit() == candidate.call.source_unit()
        && source.call().source_unit() == candidate.call.source_unit()
        && source.kind() == LoanKind::Shared
        && consumer.kind() == LoanKind::Shared
        && source.target() == consumer.target()
        && matches!(source.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == expected.root)
        && source.begin_span() == expected.source_origin
        && source.end_span() == expected.source_call_origin
        && consumer.begin_span() == expected.source_call_origin
        && consumer.end_span() == expected.consumer_call_origin
        && [
            expected.source_origin,
            expected.source_call_origin,
            expected.consumer_call_origin,
            expected.owner_origin,
        ]
        .iter()
        .all(|span| span.source_id() == file)
        && retired.transaction().ended_loans() == [consumer, source]
        && id_at(retired.source_begin(), 0)
        && id_at(retired.transfer(), 1)
        && id_at(retired.consumer_begin(), 2)
        && *point == call_return
        && drop.point() == call_return
        && drop.target() == UnitDropTarget::Named(expected.root)
        && drop.value_origin() == expected.owner_origin;
    if valid {
        UnitLoanActionCoverage::Complete
    } else {
        UnitLoanActionCoverage::Partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

    // Artificial damaged records/certificates are CI-only, never local ordinary execution.
    #[test]
    fn ci_only_consumer_coverage_rejects_incomplete_or_foreign_chains() {
        let fixture = Fixture::new(&[(
            "a.ko",
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }",
        )]);
        let shared = fixture.shared();
        let observation = &shared.loan_action_observations()[0];
        let region = observation
            .regions()
            .iter()
            .find(|region| region.protocol_version() == 4)
            .unwrap();
        let ids: Vec<_> = region.action_ids().collect();
        let start = ids[0].index;
        let all = observation.actions();
        let UnitLoanAction::Begin(source) = &all[start] else {
            unreachable!()
        };
        let UnitLoanAction::Begin(consumer) = &all[start + 2] else {
            unreachable!()
        };
        let UnitLoanAction::Cleanup {
            action: UnitIterationCleanupAction::Drop(drop),
            ..
        } = &all[start + 4]
        else {
            unreachable!()
        };
        let UnitLoanTarget::Place(place) = source.target() else {
            unreachable!()
        };
        let candidate = || Candidate {
            call: region.call(),
            callable: region.callable(),
            expected: None,
            protocol: 4,
            named: None,
            receiver: None,
            consumer: Some(ExpectedConsumer {
                source: source.clone(),
                consumer: consumer.clone(),
                root: place.root(),
                source_origin: source.begin_span(),
                source_call_origin: source.end_span(),
                consumer_call_origin: consumer.end_span(),
                owner_origin: drop.value_origin(),
                formation_span: fixture.parsed[0]
                    .ast()
                    .expressions()
                    .get(fixture.expression(0, "\"kept\""))
                    .unwrap()
                    .span(),
            }),
            start,
            end: Some(start + 5),
        };
        assert_eq!(
            coverage(&candidate(), &observation.identity, all),
            UnitLoanActionCoverage::Complete
        );
        for fault in 0..9 {
            let mut candidate = candidate();
            let mut actions = all.to_vec();
            match fault {
                0 => candidate.end = None,
                1 => {
                    actions.remove(start + 4);
                    candidate.end = Some(start + 4);
                }
                2 => {
                    actions.insert(start + 4, actions[start].clone());
                    candidate.end = Some(start + 6);
                }
                3 => actions.swap(start, start + 2),
                4 => candidate.consumer.as_mut().unwrap().owner_origin = source.begin_span(),
                5 => {
                    if let UnitLoanAction::BorrowConsumerRetired(retired) = &mut actions[start + 3]
                    {
                        retired.identity = Arc::new(());
                    }
                }
                6 => {
                    if let UnitLoanAction::BorrowConsumerRetired(retired) = &mut actions[start + 3]
                    {
                        retired.source_begin = Some(start + 2);
                    }
                }
                7 => {
                    if let UnitLoanAction::BorrowConsumerRetired(retired) = &mut actions[start + 3]
                    {
                        retired.transfer = None;
                    }
                }
                _ => {
                    if let UnitLoanAction::Cleanup { point, .. } = &mut actions[start + 4] {
                        *point = UnitDropPoint::AfterExpression(region.call());
                    }
                }
            }
            assert_eq!(
                coverage(&candidate, &observation.identity, &actions),
                UnitLoanActionCoverage::Partial
            );
        }
        let mut unsupported = candidate();
        unsupported.consumer = None;
        assert_eq!(
            coverage(&unsupported, &observation.identity, all),
            UnitLoanActionCoverage::Unsupported
        );
    }
}
