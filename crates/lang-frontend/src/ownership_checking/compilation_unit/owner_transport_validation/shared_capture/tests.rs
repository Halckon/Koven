//! CI-only corruption tests for the loan overlay independently of location replay.
use super::*;
use crate::ownership_checking::compilation_unit::{
    formed_tests::Fixture, owner_transport_tests::shared_capture::SOURCES,
};

#[test]
fn every_consuming_route_is_blocked_while_a_shared_slot_is_live() {
    let fixture = Fixture::new(&[("root.ko", SOURCES[2])]);
    let owned = fixture.shared();
    let events = owned.owner_observations()[0].events();
    assert!(consistent(events));
    let (at, expression, environment, inputs) = events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            UnitOwnerTransportEvent::SharedCapture {
                expression,
                environment,
                inputs,
            } => Some((index, *expression, environment.clone(), inputs.clone())),
            _ => None,
        })
        .unwrap();
    let (left, left_owner) = inputs[0].clone();
    let (right, right_owner) = inputs[1].clone();
    for event in [
        UnitOwnerTransportEvent::Move {
            expression,
            symbol: left,
            owner: left_owner.clone(),
        },
        UnitOwnerTransportEvent::Drop {
            point: UnitDropPoint::AfterExpression(expression),
            symbol: left,
            owner: left_owner.clone(),
        },
        UnitOwnerTransportEvent::Swap {
            call: expression,
            left,
            right,
            left_owner: left_owner.clone(),
            right_owner: right_owner.clone(),
        },
        UnitOwnerTransportEvent::Replace {
            call: expression,
            root: left,
            argument: expression,
            previous: left_owner.clone(),
            incoming: right_owner,
        },
        UnitOwnerTransportEvent::Capture {
            expression,
            environment,
            inputs,
        },
    ] {
        let mut damaged = events.to_vec();
        damaged.insert(at + 1, event.clone());
        assert!(!consistent(&damaged));
        // Overlay-only negative controls: both arms, nested as well as direct,
        // must inherit loans. Location/control authentication is checked separately.
        for in_then in [false, true] {
            let branch = |event: UnitOwnerTransportEvent| UnitOwnerTransportEvent::If {
                control: expression,
                then_events: if in_then {
                    vec![event.clone()]
                } else {
                    Vec::new()
                },
                else_events: if in_then { Vec::new() } else { vec![event] },
            };
            let mut direct = events.to_vec();
            direct.insert(at + 1, branch(event.clone()));
            assert!(!consistent(&direct));
            let mut nested = events.to_vec();
            nested.insert(at + 1, branch(branch(event.clone())));
            assert!(!consistent(&nested));
        }
    }
}

#[test]
fn ending_one_environment_does_not_end_another_environment_loan() {
    let fixture = Fixture::new(&[("root.ko", SOURCES[1])]);
    let owned = fixture.shared();
    let events = owned.owner_observations()[0].events();
    assert!(consistent(events));
    let (at, environment, source, owner, point) = events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            UnitOwnerTransportEvent::SharedCaptureEnd {
                environment,
                source,
                owner,
                point,
            } => Some((index, environment.clone(), *source, owner.clone(), *point)),
            _ => None,
        })
        .unwrap();
    assert!(
        matches!(&events[at+1], UnitOwnerTransportEvent::Drop { owner, .. } if *owner == environment)
    );
    let mut damaged = events.to_vec();
    damaged.insert(
        at + 2,
        UnitOwnerTransportEvent::Drop {
            point,
            symbol: source,
            owner,
        },
    );
    assert!(!consistent(&damaged));
    let mut interrupted = events.to_vec();
    interrupted.insert(at + 1, events[0].clone());
    assert!(!consistent(&interrupted));
    let mut missing_environment = events.to_vec();
    missing_environment.remove(at + 1);
    assert!(!consistent(&missing_environment));
}
