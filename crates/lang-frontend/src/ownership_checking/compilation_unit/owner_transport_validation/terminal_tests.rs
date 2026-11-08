//! CI-only location replay contracts, deliberately bypassing the loan overlay.
use super::*;
use crate::ownership_checking::compilation_unit::{
    formed_tests::Fixture,
    owner_transport_tests::terminal_return::{BRANCH_RETIREMENT, SOURCES, retirement_indexes},
};

fn replay(observation: &UnitOwnerObservation, events: &[UnitOwnerTransportEvent]) -> Option<bool> {
    let UnitOwnerCallable::Function(callable) = observation.callable() else {
        unreachable!()
    };
    let mut locations = observation
        .definitions()
        .iter()
        .map(|definition| match definition.origin() {
            UnitOwnerOrigin::Parameter(symbol) => Location::Named(symbol),
            _ => Location::Uncreated,
        })
        .collect::<Vec<_>>();
    let mut created = locations
        .iter()
        .map(|location| *location != Location::Uncreated)
        .collect::<Vec<_>>();
    observation.replay_owner_events(
        events,
        callable.source_unit(),
        &mut locations,
        &mut created,
        &mut Vec::new(),
        None,
    )
}

#[test]
fn location_replay_propagates_terminal_forks_and_rejects_all_successors() {
    for index in [0, 1, 4, 6] {
        let fixture = Fixture::new(&[("root.ko", SOURCES[index].0)]);
        let owned = fixture.shared();
        let observation = &owned.owner_observations()[0];
        assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
        assert_eq!(replay(observation, observation.events()), Some(false));
        let mut suffix = observation.events().to_vec();
        let UnitOwnerTransportEvent::If { else_events, .. } = suffix.last().unwrap() else {
            unreachable!()
        };
        suffix.push(else_events.last().unwrap().clone());
        assert_eq!(replay(observation, &suffix), None);
    }
    let fixture = Fixture::new(&[("root.ko", SOURCES[4].0)]);
    let owned = fixture.shared();
    let observation = &owned.owner_observations()[0];
    let mut nested_suffix = observation.events().to_vec();
    let UnitOwnerTransportEvent::If {
        then_events,
        else_events,
        ..
    } = nested_suffix.last_mut().unwrap()
    else {
        unreachable!()
    };
    then_events.push(else_events.last().unwrap().clone());
    assert_eq!(replay(observation, &nested_suffix), None);
}

#[test]
fn location_replay_requires_actual_terminal_retirement_on_both_arms() {
    let fixture = Fixture::new(&[("root.ko", BRANCH_RETIREMENT)]);
    let owned = fixture.shared();
    let observation = &owned.owner_observations()[0];
    assert_eq!(replay(observation, observation.events()), Some(false));
    for arm in [0, 1] {
        for remove_environment in [true, false] {
            let mut events = observation.events().to_vec();
            let UnitOwnerTransportEvent::If {
                then_events,
                else_events,
                ..
            } = events.last_mut().unwrap()
            else {
                unreachable!()
            };
            let edge = if arm == 0 { then_events } else { else_events };
            let (_, environment_drop, source_drop) = retirement_indexes(observation, edge);
            edge.remove(if remove_environment {
                environment_drop
            } else {
                source_drop
            });
            assert_eq!(replay(observation, &events), None);
        }
    }
}
