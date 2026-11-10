//! Ordinary cross-source evidence plus CI-only broken-model validation guards.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

const CALLEE: &str = "fun inspect(a: String, b: String): Int = 1";
const CALLER: &str = "fun run(): Unit { inspect(\"first\", \"second\") }";

#[test]
fn loan_action_region_cross_source_keeps_caller_and_parameter_lineage() {
    for sources in [
        [("a.ko", CALLEE), ("b.ko", CALLER)],
        [("b.ko", CALLER), ("a.ko", CALLEE)],
    ] {
        let fixture = Fixture::new(&sources);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let observation = shared
            .loan_action_observations()
            .iter()
            .find(|observation| !observation.regions().is_empty())
            .unwrap();
        let [region] = observation.regions() else {
            panic!("one caller region")
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.call().source_unit(), observation.source());
        assert_eq!(
            region.callable().unwrap().source_unit(),
            observation.source()
        );
        let other = shared
            .loan_action_observations()
            .iter()
            .find(|candidate| candidate.source() != observation.source())
            .unwrap();
        for id in region.action_ids() {
            assert!(other.action(&id).is_none());
            if let Some(UnitLoanAction::Begin(loan)) = observation.action(&id) {
                assert_ne!(
                    loan.parameter_span().unwrap().source_id(),
                    loan.begin_span().source_id()
                );
                assert_eq!(loan.argument().source_unit(), observation.source());
            }
        }
    }
}

// These deliberately corrupted products are CI-only. Local exact ordinary selections
// must not execute this test; all-target compilation is not a claimed passing run.
#[test]
fn ci_only_broken_region_models_do_not_certify_complete() {
    let fixture = Fixture::new(&[("a.ko", CALLEE), ("b.ko", CALLER)]);
    let shared = fixture.shared();
    let observation = shared
        .loan_action_observations()
        .iter()
        .find(|observation| !observation.regions().is_empty())
        .unwrap();
    let region = &observation.regions()[0];
    let actions: Vec<_> = region
        .action_ids()
        .map(|id| observation.action(&id).unwrap().clone())
        .collect();
    let expected: Vec<_> = actions
        .iter()
        .filter_map(|action| {
            let UnitLoanAction::Begin(loan) = action else {
                return None;
            };
            let node = fixture.parsed[1]
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .unwrap();
            Some(Expected {
                loan: loan.clone(),
                origin: node.span(),
            })
        })
        .collect();
    assert_eq!(actions.len(), 6);
    assert!(validates(region.call(), &expected, &actions));
    let mut broken = actions.clone();
    broken.swap(0, 2); // End before all operands acquired.
    assert!(!validates(region.call(), &expected, &broken));
    let mut broken = actions.clone();
    broken.remove(0); // Missing acquisition.
    assert!(!validates(region.call(), &expected, &broken));
    let mut broken = actions.clone();
    broken[2] = broken[3].clone(); // Duplicate one End, omit its sibling.
    assert!(!validates(region.call(), &expected, &broken));
    let mut broken = actions.clone();
    broken.swap(4, 5); // Wrong temporary destruction order.
    assert!(!validates(region.call(), &expected, &broken));
    let mut broken = actions.clone();
    if let UnitLoanAction::Cleanup { point, .. } = &mut broken[2] {
        *point = super::super::UnitDropPoint::AfterExpression(region.call());
    }
    assert!(!validates(region.call(), &expected, &broken));
    let mut broken = actions.clone();
    broken.push(actions[0].clone());
    assert!(!validates(region.call(), &expected, &broken));
    let mut broken = actions.clone();
    broken.pop();
    assert!(!validates(region.call(), &expected, &broken));
    let mut broken = actions.clone();
    broken.swap(2, 4);
    assert!(!validates(region.call(), &expected, &broken));
    let mut wrong_origin = expected.clone();
    wrong_origin[0].origin = expected[1].origin;
    assert!(!validates(region.call(), &wrong_origin, &actions));
    let wrong_callable = Candidate {
        receiver_protocol: false,
        receiver: None,
        call: region.call(),
        callable: Some(UnitItemId::new(
            fixture.source(0),
            fixture.parsed[0].roots()[0],
        )),
        expected: Some(expected.clone()),
        start: 0,
        end: Some(actions.len()),
    };
    assert_eq!(
        wrong_callable.finish(Arc::new(()), &actions).coverage(),
        UnitLoanActionCoverage::Partial
    );
    let absent_completion = Candidate {
        receiver_protocol: false,
        receiver: None,
        call: region.call(),
        callable: region.callable(),
        expected: Some(expected),
        start: 0,
        end: None,
    };
    assert_eq!(
        absent_completion.finish(Arc::new(()), &actions).coverage(),
        UnitLoanActionCoverage::Partial
    );
}

#[test]
fn ci_only_empty_region_requires_all_positive_boundary_evidence() {
    let fixture = Fixture::new(&[(
        "zero.ko",
        "fun number(): Int = 1\nfun run(): Int = number()",
    )]);
    let shared = fixture.shared();
    let region = &shared.loan_action_observations()[0].regions()[0];
    let candidate = |expected, start, end| Candidate {
        receiver_protocol: false,
        receiver: None,
        call: region.call(),
        callable: region.callable(),
        expected,
        start,
        end,
    };
    assert_eq!(
        candidate(Some(Vec::new()), 0, None)
            .finish(Arc::new(()), &[])
            .coverage(),
        UnitLoanActionCoverage::Partial
    );
    assert_eq!(
        candidate(None, 0, Some(0))
            .finish(Arc::new(()), &[])
            .coverage(),
        UnitLoanActionCoverage::Unsupported
    );
    assert_eq!(
        candidate(Some(Vec::new()), 1, Some(2))
            .finish(Arc::new(()), &[])
            .coverage(),
        UnitLoanActionCoverage::Partial
    );
    let nonempty = Fixture::new(&[("a.ko", CALLEE), ("b.ko", CALLER)]).shared();
    let observation = nonempty
        .loan_action_observations()
        .iter()
        .find(|observation| !observation.actions().is_empty())
        .unwrap();
    assert!(!validates(region.call(), &[], observation.actions()));
}

#[test]
fn loan_receiver_cross_source_retains_declaration_and_caller_identity() {
    let fixture = Fixture::new(&[
        ("a.ko", "class Node { fun touch(): Int = 1 }"),
        ("b.ko", "fun run(own node: Node): Unit { node.touch() }"),
    ]);
    let shared = fixture.shared();
    assert!(shared.clone().validate().is_ok());
    let observation = shared
        .loan_action_observations()
        .iter()
        .find(|entry| !entry.regions().is_empty())
        .unwrap();
    let region = &observation.regions()[0];
    assert_eq!(region.protocol_version(), 2);
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    let other = shared
        .loan_action_observations()
        .iter()
        .find(|entry| entry.source() != observation.source())
        .unwrap();
    assert!(!other.owns_region(region));
    for id in region.action_ids() {
        assert!(other.action(&id).is_none());
        if let Some(UnitLoanAction::ReceiverBegin(receiver)) = observation.action(&id) {
            assert_eq!(receiver.call().source_unit(), observation.source());
            assert_ne!(
                receiver.begin_span().source_id(),
                receiver.declaration_span().unwrap().source_id()
            );
        }
    }
}

// Deliberately broken validation products: compile locally, execute only in CI.
#[test]
fn ci_only_broken_receiver_region_intervals_do_not_certify_complete() {
    use super::super::{UnitIterationCleanupAction, UnitReceiverOwnershipTarget};
    let fixture = Fixture::new(&[(
        "a.ko",
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch() }",
    )]);
    let shared = fixture.shared();
    let observation = &shared.loan_action_observations()[0];
    let region = &observation.regions()[0];
    let actions: Vec<_> = region
        .action_ids()
        .map(|id| observation.action(&id).unwrap().clone())
        .collect();
    let [
        UnitLoanAction::ReceiverBegin(receiver),
        _,
        UnitLoanAction::Cleanup {
            action: UnitIterationCleanupAction::Drop(drop),
            ..
        },
    ] = actions.as_slice()
    else {
        panic!("real complete receiver");
    };
    let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
        panic!("real owner");
    };
    let mut candidate = Candidate {
        call: region.call(),
        callable: region.callable(),
        expected: None,
        receiver_protocol: true,
        receiver: Some(ExpectedReceiver {
            receiver: receiver.clone(),
            root: place.root(),
            receiver_origin: receiver.begin_span(),
            owner_origin: drop.value_origin(),
        }),
        start: 0,
        end: Some(3),
    };
    assert_eq!(
        receiver_coverage(&candidate, &actions),
        UnitLoanActionCoverage::Complete
    );
    let mut broken = actions.clone();
    broken.swap(0, 1);
    assert_eq!(
        receiver_coverage(&candidate, &broken),
        UnitLoanActionCoverage::Partial
    );
    let mut broken = actions.clone();
    broken[2] = broken[1].clone();
    assert_eq!(
        receiver_coverage(&candidate, &broken),
        UnitLoanActionCoverage::Partial
    );
    assert_eq!(
        receiver_coverage(&candidate, &actions[..2]),
        UnitLoanActionCoverage::Partial
    );
    candidate.receiver.as_mut().unwrap().owner_origin = receiver.begin_span();
    assert_eq!(
        receiver_coverage(&candidate, &actions),
        UnitLoanActionCoverage::Partial
    );
    candidate.receiver.as_mut().unwrap().owner_origin = drop.value_origin();
    candidate.end = None;
    assert_eq!(
        receiver_coverage(&candidate, &actions),
        UnitLoanActionCoverage::Partial
    );
    candidate.end = Some(3);
    candidate.callable = None;
    assert_eq!(
        receiver_coverage(&candidate, &actions),
        UnitLoanActionCoverage::Partial
    );
    candidate.receiver = None;
    assert_eq!(
        receiver_coverage(&candidate, &actions),
        UnitLoanActionCoverage::Unsupported
    );
}
