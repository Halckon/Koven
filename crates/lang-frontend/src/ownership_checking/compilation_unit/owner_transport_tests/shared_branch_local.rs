//! A branch-local environment has a real definition but never needs an outgoing phi.
use super::shared_capture::paths;
use super::*;

pub(in crate::ownership_checking::compilation_unit) const SOURCES: &[&str] = &[
    r#"fun run(flag: Boolean): Unit { val text = "x"; if (flag) { val callback = { text == "x" }; val moved = callback } else {} }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; if (flag) { val a = { text == "a" }; val moved = a } else { val b = { text == "b" }; val moved = b } }"#,
    r#"fun run(flag: Boolean): Unit { if (flag) { val text = "x"; val a = { text == "a" }; val moved = a } else {} }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; if (flag) { if (flag) { val a = { text == "a" }; val moved = a } else { val b = { text == "b" }; val moved = b } } else {} }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val outer = { text == "outer" }; if (flag) { val inner = { text == "inner" }; val moved = inner } else {}; val retained = outer }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val other = "y"; if (flag) { val a = { text == other }; val b = { text == other }; val moved_a = a; val moved_b = b } else {} }"#,
];

pub(in crate::ownership_checking::compilation_unit) const BOUNDARIES: &[&str] = &[
    r#"fun run(flag: Boolean): Unit { val text = "x"; val other = "y"; var callback = { text == "x" }; if (flag) { callback = { other == "y" } } else {}; val moved = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; if (flag) { val a = move { text }; val moved = a } else {} }"#,
    r#"fun run(flag: Boolean, number: Int): Unit { val text = "x"; if (flag) { val a = { text == "x" && number == 0 }; val moved = a } else {} }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; if (flag) { val a = { text == "x" }; val moved = a } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; while (flag) { val a = { text == "x" }; val moved = a } }"#,
];

fn check_local_edges(
    events: &[UnitOwnerTransportEvent],
    observation: &UnitOwnerObservation,
) -> usize {
    let mut formations = 0;
    for event in events {
        let UnitOwnerTransportEvent::If {
            then_events,
            else_events,
            ..
        } = event
        else {
            continue;
        };
        for edge in [then_events, else_events] {
            for (at, event) in edge.iter().enumerate() {
                let UnitOwnerTransportEvent::SharedCapture {
                    expression,
                    environment,
                    inputs,
                } = event
                else {
                    continue;
                };
                formations += 1;
                assert_eq!(
                    observation
                        .definition(environment.clone())
                        .unwrap()
                        .origin(),
                    UnitOwnerOrigin::Closure(*expression)
                );
                for path in paths(&edge[at + 1..]) {
                    let ends = path
                        .iter()
                        .enumerate()
                        .filter_map(|(index, event)| match event {
                            UnitOwnerTransportEvent::SharedCaptureEnd {
                                environment: actual,
                                source,
                                owner,
                                point,
                            } if actual == environment => Some((index, *source, owner, point)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(ends.len(), inputs.len());
                    for (end, input) in ends.iter().zip(inputs.iter().rev()) {
                        assert_eq!(end.1, input.0);
                        assert_eq!(end.2, &input.1);
                        assert_eq!(end.3, ends[0].3);
                    }
                    assert!(ends.windows(2).all(|pair| pair[0].0 + 1 == pair[1].0));
                    let last = ends.last().unwrap();
                    assert!(
                        matches!(path[last.0 + 1], UnitOwnerTransportEvent::Drop { owner, point, .. } if owner == environment && point == last.3)
                    );
                }
            }
            formations += check_local_edges(edge, observation);
        }
    }
    formations
}

#[test]
fn shared_capture_formed_and_retired_inside_if_is_observed() {
    for (index, text) in SOURCES.iter().enumerate() {
        let owned = checked(text);
        let observation = &owned.owner_observations()[0];
        assert_eq!(
            observation.status(),
            UnitOwnerObservationStatus::Complete,
            "{text}"
        );
        assert!(observation.is_consistent());
        assert!(observation.value_observation_v1().is_none());
        assert_eq!(
            check_local_edges(observation.events(), observation),
            [1, 2, 1, 2, 1, 2][index]
        );
        // Every observed source token exists, with each environment released before its source.
        for path in paths(observation.events()) {
            for event in &path {
                let UnitOwnerTransportEvent::SharedCapture {
                    environment,
                    inputs,
                    ..
                } = event
                else {
                    continue;
                };
                let end = path.iter().position(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == environment)).unwrap();
                for (_, source) in inputs {
                    assert!(observation.definition(source.clone()).is_some());
                    let source_end = path.iter().position(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == source)).unwrap();
                    assert!(source_end > end);
                }
            }
        }
    }
}

#[test]
fn branch_local_shared_capture_does_not_admit_escaping_or_other_kinds() {
    for text in BOUNDARIES {
        let owned = checked(text);
        let observation = &owned.owner_observations()[0];
        assert!(
            matches!(
                observation.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ),
            "{text}"
        );
        assert!(observation.events().is_empty());
        assert!(observation.definitions().is_empty());
    }
}

// CI-only malformed-product controls. Local runs compile these but do not execute them.
#[test]
fn branch_local_environment_definition_cannot_be_formed_on_sibling_edges() {
    let owned = checked(SOURCES[0]);
    let mut damaged = owned.owner_observations()[0].clone();
    let UnitOwnerTransportEvent::If {
        then_events,
        else_events,
        ..
    } = damaged
        .events
        .iter_mut()
        .find(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    *else_events = then_events.clone();
    // Retag genuine edge-local cleanup so a wrong branch number cannot mask
    // the duplicate static definition. Only this environment is duplicated.
    for event in else_events {
        let point = match event {
            UnitOwnerTransportEvent::SharedCaptureEnd { point, .. }
            | UnitOwnerTransportEvent::Drop { point, .. } => point,
            _ => continue,
        };
        if let UnitDropPoint::BranchEntry { branch, .. }
        | UnitDropPoint::BranchExit { branch, .. } = point
        {
            *branch = 1;
        }
    }
    assert!(!damaged.is_consistent());
}

#[test]
fn branch_local_shared_capture_requires_complete_retirement() {
    let owned = checked(SOURCES[0]);
    let original = &owned.owner_observations()[0];
    for damage in 0..4 {
        let mut damaged = original.clone();
        let UnitOwnerTransportEvent::If { then_events, .. } = damaged
            .events
            .iter_mut()
            .find(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        let at = then_events
            .iter()
            .position(|event| matches!(event, UnitOwnerTransportEvent::SharedCaptureEnd { .. }))
            .unwrap();
        match damage {
            0 => {
                then_events.remove(at);
            }
            1 => {
                then_events.remove(at + 1);
            }
            2 => {
                then_events.insert(at, then_events[at].clone());
            }
            _ => {
                let UnitOwnerTransportEvent::SharedCaptureEnd {
                    source,
                    owner,
                    point,
                    ..
                } = &then_events[at]
                else {
                    unreachable!()
                };
                let drop = UnitOwnerTransportEvent::Drop {
                    symbol: *source,
                    owner: owner.clone(),
                    point: *point,
                };
                then_events.insert(at, drop);
            }
        }
        assert!(!damaged.is_consistent());
    }
}
