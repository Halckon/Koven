//! Branch release consumes slots of an environment formed before the fork.
use super::combination::BRANCH_RELEASE;
use super::*;

pub(in crate::ownership_checking::compilation_unit) const NESTED_RELEASE: &str = r#"fun run(flag: Boolean, own text: String): Unit {
val callback = move { text }
if (flag) {
    if (flag) { val first = callback } else { val second = callback }
} else { val third = callback }
}"#;

pub(in crate::ownership_checking::compilation_unit) const ASYMMETRIC_RELEASE: &str = r#"fun run(flag: Boolean, own text: String): Unit {
val callback = move { text }
if (flag) { val left = callback } else { val n = 1 }
}"#;

pub(in crate::ownership_checking::compilation_unit) const RETURN_RELEASE: &str = r#"fun run(flag: Boolean, own text: String): Unit {
val callback = move { text }
if (flag) { val first = callback; return } else { val second = callback }
}"#;

fn function(owned: &CompilationUnitOwnership) -> &UnitOwnerObservation {
    owned
        .owner_observations()
        .iter()
        .find(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
        .unwrap()
}

fn release_paths(
    events: &[UnitOwnerTransportEvent],
    environment: &UnitOwnerId,
    input: &(crate::name_resolution::UnitSymbolId, UnitOwnerId),
) -> usize {
    let mut paths = 0;
    for event in events {
        if let UnitOwnerTransportEvent::If {
            then_events,
            else_events,
            ..
        } = event
        {
            for edge in [then_events, else_events] {
                if edge
                    .iter()
                    .any(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
                {
                    paths += release_paths(edge, environment, input);
                    continue;
                }
                let releases = edge
                    .iter()
                    .enumerate()
                    .filter_map(|(index, event)| match event {
                        UnitOwnerTransportEvent::CaptureDrop {
                            environment: actual,
                            source,
                            owner,
                            ..
                        } => {
                            assert_eq!(actual, environment);
                            assert_eq!(*source, input.0);
                            assert_eq!(owner, &input.1);
                            Some(index)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [release] = releases.as_slice() else {
                    panic!("one slot release per real path")
                };
                let environment_drop = edge
                    .iter()
                    .position(|event| {
                        matches!(event,
                    UnitOwnerTransportEvent::Drop { owner, .. } if owner == environment)
                    })
                    .unwrap();
                assert!(
                    *release < environment_drop,
                    "release the actual slot before its environment"
                );
                assert!(
                    !edge
                        .iter()
                        .any(|event| matches!(event, UnitOwnerTransportEvent::Capture { .. }))
                );
                paths += 1;
            }
        }
    }
    paths
}

#[test]
fn real_edges_release_the_same_environment_and_slot_once_per_path() {
    for (text, expected_paths) in [
        (BRANCH_RELEASE, 2),
        (NESTED_RELEASE, 3),
        (ASYMMETRIC_RELEASE, 2),
    ] {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observation = function(&owned);
        assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
        assert!(observation.is_consistent());
        let formations = observation
            .events()
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::Capture {
                    environment,
                    inputs,
                    ..
                } => Some((environment, inputs)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(environment, inputs)] = formations.as_slice() else {
            panic!("one top-level formation")
        };
        let [input] = inputs.as_slice() else {
            panic!("one captured String")
        };
        assert_eq!(
            release_paths(observation.events(), environment, input),
            expected_paths
        );
    }
}

#[test]
fn release_replay_rejects_wrong_edge_missing_and_repeated_slot_release() {
    let fixture = Fixture::new(&[("root.ko", BRANCH_RELEASE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    for mutation in ["wrong edge", "missing release", "duplicate release"] {
        let mut damaged = observation.clone();
        let (control, edge) = damaged
            .events
            .iter_mut()
            .find_map(|event| match event {
                UnitOwnerTransportEvent::If {
                    control,
                    then_events,
                    ..
                } => Some((*control, then_events)),
                _ => None,
            })
            .unwrap();
        let index = edge
            .iter()
            .position(|event| matches!(event, UnitOwnerTransportEvent::CaptureDrop { .. }))
            .unwrap();
        match mutation {
            "wrong edge" => {
                let UnitOwnerTransportEvent::CaptureDrop { point, .. } = &mut edge[index] else {
                    unreachable!()
                };
                *point = UnitDropPoint::BranchExit { control, branch: 1 };
            }
            "missing release" => {
                edge.remove(index);
            }
            "duplicate release" => {
                edge.insert(index, edge[index].clone());
            }
            _ => unreachable!(),
        }
        assert!(!damaged.is_consistent(), "{mutation}");
    }
}

// Both real edges terminate: this remains outside the single-return admission.
pub(in crate::ownership_checking::compilation_unit) const BOTH_RETURN_RELEASE: &str = r#"fun run(flag: Boolean, own text: String): Unit {
val callback = move { text }
if (flag) { val first = callback; return } else { val second = callback; return }
}"#;

#[test]
fn capture_release_admits_one_bare_return_without_changing_owned_slots() {
    let fixture = Fixture::new(&[("root.ko", RETURN_RELEASE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned, fixture.shared());
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    assert!(observation.value_observation_v1().is_none());
    let formations = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Capture {
                expression,
                environment,
                inputs,
            } => Some((*expression, environment, inputs)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(expression, environment, inputs)] = formations.as_slice() else {
        panic!("one pre-fork environment")
    };
    assert_eq!(
        *expression,
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "move { text }"))
    );
    assert_eq!(
        observation
            .definition((*environment).clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::Closure(*expression)
    );
    let [input] = inputs.as_slice() else {
        panic!("one owned String slot")
    };
    assert_eq!(input.0, fixture.symbol(0, "text"));
    assert_eq!(
        observation.definition(input.1.clone()).unwrap().origin(),
        UnitOwnerOrigin::Parameter(input.0)
    );
    assert_eq!(release_paths(observation.events(), environment, input), 2);
    let forks = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::If {
                control,
                then_events,
                else_events,
            } => Some((*control, then_events, else_events)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(control, then_events, else_events)] = forks.as_slice() else {
        panic!("one real fork")
    };
    assert_eq!(
        *control,
        UnitExpressionId::new(
            fixture.source(0),
            fixture.expression(
                0,
                "if (flag) { val first = callback; return } else { val second = callback }"
            )
        )
    );
    let returned = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "return"));
    assert!(
        matches!(then_events.last(), Some(UnitOwnerTransportEvent::UnitReturn { expression }) if *expression == returned)
    );
    assert_eq!(
        then_events
            .iter()
            .filter(|event| matches!(event, UnitOwnerTransportEvent::UnitReturn { .. }))
            .count(),
        1
    );
    assert!(
        !else_events
            .iter()
            .any(|event| matches!(event, UnitOwnerTransportEvent::UnitReturn { .. }))
    );
    for (edge, alias) in [(then_events, "first"), (else_events, "second")] {
        let binding = fixture.symbol(0, alias);
        // These unused aliases retire before return/branch exit at their own AST statement.
        let expected = super::capture_composition::local_drop_point(&fixture, binding);
        let releases = edge
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::CaptureDrop { point, .. } => Some(*point),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(releases, vec![expected]);
        let drops = edge
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::Drop {
                    symbol,
                    owner,
                    point,
                } => Some((*symbol, owner, *point)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(drops, vec![(binding, *environment, expected)]);
        assert!(edge.iter().any(|event| matches!(event, UnitOwnerTransportEvent::Binding { symbol, owner, .. } if *symbol == binding && owner == *environment)));
    }
    assert!(!observation.events().iter().any(|event| matches!(
        event,
        UnitOwnerTransportEvent::Drop { .. }
            | UnitOwnerTransportEvent::CaptureDrop { .. }
            | UnitOwnerTransportEvent::UnitReturn { .. }
    )));
}

#[test]
fn capture_release_does_not_admit_noncontinuing_if_edges() {
    let fixture = Fixture::new(&[("root.ko", BOTH_RETURN_RELEASE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    assert_eq!(
        observation.status(),
        UnitOwnerObservationStatus::Unsupported {
            point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                fixture.source(0),
                fixture.expression(
                    0,
                    "if (flag) { val first = callback; return } else { val second = callback; return }"
                ),
            )),
            reason: UnitOwnerUnsupportedReason::Syntax,
        }
    );
    assert!(observation.definitions().is_empty());
    assert!(observation.events().is_empty());
    assert!(observation.is_consistent());
}

#[test]
fn implicit_release_on_the_unused_edge_keeps_its_real_branch_exit_identity() {
    let fixture = Fixture::new(&[("root.ko", ASYMMETRIC_RELEASE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let expected_control = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "if (flag) { val left = callback } else { val n = 1 }"),
    );
    let (control, else_events) = observation
        .events()
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::If {
                control,
                else_events,
                ..
            } => Some((*control, else_events)),
            _ => None,
        })
        .unwrap();
    assert_eq!(control, expected_control);
    let points = else_events
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::CaptureDrop { point, .. } => Some(*point),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        points,
        vec![UnitDropPoint::BranchExit { control, branch: 1 }]
    );
}
