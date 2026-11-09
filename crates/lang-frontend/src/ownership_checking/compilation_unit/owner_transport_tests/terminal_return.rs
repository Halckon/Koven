//! Every terminal alternative releases its actual instances without inventing a join.
use super::shared_return::paths;
use super::*;

// Real branch-local alias uses keep the incoming capture live through the fork.
// Cleanup belongs to the alias's last use, not an invented Return-only endpoint.
pub(in crate::ownership_checking::compilation_unit) const BRANCH_RETIREMENT: &str = r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "probe" }; if (flag) { val left = callback; return } else { val right = callback; return } }"#;

// Keep the original unsupported Shared and Owned sources byte-for-byte as positive witnesses.
pub(in crate::ownership_checking::compilation_unit) const SOURCES: &[(&str, usize, usize)] = &[
    (
        r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { return } else { return } }"#,
        2,
        0,
    ),
    (super::capture_branch_release::BOTH_RETURN_RELEASE, 2, 0),
    (
        r#"fun run(flag: Boolean): Unit { val text = "x"; val first = { text == "x" }; val second = { text == "y" }; if (flag) { return } else { return } }"#,
        2,
        0,
    ),
    (
        r#"fun run(flag: Boolean): Unit { val text = "x"; val other = "y"; val callback = { text == other }; if (flag) { return } else { return } }"#,
        2,
        0,
    ),
    (
        r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { if (flag) { return } else { return } } else { return } }"#,
        3,
        0,
    ),
    (
        r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { if (flag) { return } else { return } } else {}; val forwarded = callback }"#,
        2,
        1,
    ),
    (
        r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { val local = "left"; val inner = { local == "probe" }; val moved = inner; return } else { val other = "right"; return } }"#,
        2,
        0,
    ),
    (
        r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) {} else { if (flag) { val local = "left"; val inner = { local == "probe" }; val moved = inner; return } else { return } }; val forwarded = callback }"#,
        2,
        1,
    ),
    (BRANCH_RETIREMENT, 2, 0),
];

fn control_ids(events: &[UnitOwnerTransportEvent], ids: &mut Vec<UnitExpressionId>) {
    for event in events {
        if let UnitOwnerTransportEvent::If {
            control,
            then_events,
            else_events,
        } = event
        {
            ids.push(*control);
            control_ids(then_events, ids);
            control_ids(else_events, ids);
        }
    }
}

#[test]
fn terminal_if_paths_retire_each_actual_owner_and_capture_once() {
    for &(text, terminal_count, continuing_count) in SOURCES {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        assert_eq!(owned, fixture.shared());
        let observation = &owned.owner_observations()[0];
        assert_eq!(
            observation.status(),
            UnitOwnerObservationStatus::Complete,
            "{text}"
        );
        assert!(observation.is_consistent());
        assert!(observation.value_observation_v1().is_none());
        let expected_returns = fixture.parsed[0]
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(
                    node.payload(),
                    crate::parser::Expression::Return { value: None, .. }
                )
                .then_some(UnitExpressionId::new(fixture.source(0), id))
            })
            .collect::<Vec<_>>();
        let expected_controls = fixture.parsed[0]
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), crate::parser::Expression::If { .. })
                    .then_some(UnitExpressionId::new(fixture.source(0), id))
            })
            .collect::<Vec<_>>();
        let mut actual_controls = Vec::new();
        control_ids(observation.events(), &mut actual_controls);
        assert_eq!(actual_controls.len(), expected_controls.len());
        for expected in expected_controls {
            assert_eq!(
                actual_controls
                    .iter()
                    .filter(|actual| **actual == expected)
                    .count(),
                1,
                "each real If keeps its own source-qualified identity"
            );
        }
        let alternatives = paths(observation.events());
        assert_eq!(alternatives.len(), terminal_count + continuing_count);
        let mut actual_returns = Vec::new();
        for path in alternatives {
            if let Some(UnitOwnerTransportEvent::UnitReturn { expression }) = path.last() {
                actual_returns.push(*expression);
                assert_eq!(
                    path.iter()
                        .filter(|event| matches!(event, UnitOwnerTransportEvent::UnitReturn { .. }))
                        .count(),
                    1
                );
            }
            for definition in observation.definitions() {
                let owner = definition.id();
                let formed = match definition.origin() {
                    UnitOwnerOrigin::Parameter(_) => true,
                    UnitOwnerOrigin::String(origin) => path.iter().any(|event| matches!(event,
                        UnitOwnerTransportEvent::Result { expression, owner: actual }
                        if *expression == origin && *actual == owner)),
                    UnitOwnerOrigin::Closure(_) => path.iter().any(|event| matches!(event,
                        UnitOwnerTransportEvent::SharedCapture { environment, .. }
                        | UnitOwnerTransportEvent::Capture { environment, .. } if *environment == owner)),
                };
                let cleanup = path.iter().filter(|event| matches!(event,
                    UnitOwnerTransportEvent::Drop { owner: actual, .. }
                    | UnitOwnerTransportEvent::CaptureDrop { owner: actual, .. } if *actual == owner)).count();
                assert_eq!(
                    cleanup,
                    usize::from(formed),
                    "one actual lifetime per path: {text}"
                );
            }
            for event in &path {
                let UnitOwnerTransportEvent::SharedCapture {
                    environment,
                    inputs,
                    ..
                } = event
                else {
                    continue;
                };
                let ends = path
                    .iter()
                    .enumerate()
                    .filter_map(|(index, event)| match event {
                        UnitOwnerTransportEvent::SharedCaptureEnd {
                            environment: actual,
                            source,
                            owner,
                            point,
                        } if actual == environment => Some((index, *source, owner, *point)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(ends.len(), inputs.len());
                for (end, input) in ends.iter().zip(inputs.iter().rev()) {
                    assert_eq!((end.1, end.2), (input.0, &input.1));
                    if matches!(end.3, UnitDropPoint::ControlTransfer(_)) {
                        assert!(
                            matches!(path.last(), Some(UnitOwnerTransportEvent::UnitReturn { expression })
                            if end.3 == UnitDropPoint::ControlTransfer(*expression))
                        );
                    }
                }
                assert!(ends.windows(2).all(|pair| pair[0].0 + 1 == pair[1].0));
                let end = ends.last().unwrap();
                assert!(
                    matches!(path[end.0 + 1], UnitOwnerTransportEvent::Drop { owner, point, .. }
                    if owner == environment && *point == end.3)
                );
                for (_, source) in inputs {
                    let source_drop = path
                        .iter()
                        .position(|event| {
                            matches!(event,
                        UnitOwnerTransportEvent::Drop { owner, .. } if owner == source)
                        })
                        .unwrap();
                    assert!(source_drop > end.0 + 1);
                }
            }
        }
        assert_eq!(actual_returns.len(), terminal_count);
        assert_eq!(
            actual_returns, expected_returns,
            "return identity and source order"
        );
    }
}

#[test]
fn terminal_owned_capture_preserves_original_alias_cleanup_points() {
    let fixture = Fixture::new(&[(
        "root.ko",
        super::capture_branch_release::BOTH_RETURN_RELEASE,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    let (environment, inputs) = observation
        .events()
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::Capture {
                environment,
                inputs,
                ..
            } => Some((environment, inputs)),
            _ => None,
        })
        .unwrap();
    let [input] = inputs.as_slice() else {
        panic!("one original owned input")
    };
    assert_eq!(input.0, fixture.symbol(0, "text"));
    assert_eq!(
        observation.definition(input.1.clone()).unwrap().origin(),
        UnitOwnerOrigin::Parameter(input.0)
    );
    let alternatives = paths(observation.events());
    assert_eq!(alternatives.len(), 2);
    for (path, alias) in alternatives.iter().zip(["first", "second"]) {
        let symbol = fixture.symbol(0, alias);
        let point = super::capture_composition::local_drop_point(&fixture, symbol);
        let releases = path
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::CaptureDrop {
                    environment: actual,
                    source,
                    owner,
                    point: at,
                } => {
                    assert_eq!(
                        (actual, *source, owner, *at),
                        (environment, input.0, &input.1, point)
                    );
                    Some(())
                }
                _ => None,
            })
            .count();
        assert_eq!(releases, 1);
        let drops = path
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::Drop {
                    owner,
                    symbol: actual,
                    point: at,
                } => Some((owner, *actual, *at)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(drops, vec![(environment, symbol, point)]);
    }
}

pub(in crate::ownership_checking::compilation_unit) const BOUNDARIES: &[&str] = &[
    r#"fun run(flag: Boolean): String { val text = "x"; if (flag) { return text } else { return "right" } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { return }; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; while (flag) { if (flag) { return } else { return } }; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; if (flag) { val callback = move { text }; return } else { return } }"#,
];

#[test]
fn terminal_if_keeps_payload_implicit_loop_and_owned_branch_boundaries_closed() {
    for source in BOUNDARIES {
        let owned = checked(source);
        let observation = &owned.owner_observations()[0];
        assert!(
            matches!(
                observation.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ),
            "{source}"
        );
        assert!(observation.definitions().is_empty());
        assert!(observation.events().is_empty());
        assert!(observation.value_observation_v1().is_none());
        assert!(observation.is_consistent());
    }
}

// CI-only damaged-product contracts; local verification compiles but does not execute them.
#[test]
fn terminal_if_replay_rejects_suffixes_missing_cleanup_and_duplicate_retirement() {
    let owned = checked(BRANCH_RETIREMENT);
    let original = &owned.owner_observations()[0];
    assert_eq!(original.status(), UnitOwnerObservationStatus::Complete);
    assert!(original.is_consistent());
    for damage in 0..6 {
        let mut damaged = original.clone();
        let UnitOwnerTransportEvent::If { then_events, .. } = damaged.events.last_mut().unwrap()
        else {
            unreachable!()
        };
        let (end, environment_drop, source_drop) = retirement_indexes(original, then_events);
        let exit = then_events.last().unwrap().clone();
        match damage {
            0 => {
                then_events.remove(end);
            }
            1 => {
                then_events.remove(environment_drop);
            }
            2 => {
                then_events.remove(source_drop);
            }
            3 => {
                then_events.insert(end, then_events[end].clone());
            }
            4 => {
                then_events.push(exit);
            }
            _ => {
                damaged.events.push(exit);
            }
        }
        assert!(!damaged.is_consistent(), "damage {damage}");
    }
}

#[test]
fn terminal_if_nested_exit_cannot_authorize_an_enclosing_suffix() {
    let owned = checked(SOURCES[4].0);
    let original = &owned.owner_observations()[0];
    assert_eq!(original.status(), UnitOwnerObservationStatus::Complete);
    let mut damaged = original.clone();
    let UnitOwnerTransportEvent::If {
        then_events,
        else_events,
        ..
    } = damaged.events.last_mut().unwrap()
    else {
        unreachable!()
    };
    // Each nested child returns; appending even another terminal event is unreachable.
    then_events.push(else_events.last().unwrap().clone());
    assert!(!damaged.is_consistent());
}

// Assert actual mutation targets before editing a copy, independently of either validator.
pub(in crate::ownership_checking::compilation_unit) fn retirement_indexes(
    observation: &UnitOwnerObservation,
    edge: &[UnitOwnerTransportEvent],
) -> (usize, usize, usize) {
    let formations = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::SharedCapture {
                environment,
                inputs,
                ..
            } => Some((environment, inputs)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(environment, inputs)] = formations.as_slice() else {
        panic!("one incoming environment")
    };
    let [(source, owner)] = inputs.as_slice() else {
        panic!("one actual shared input")
    };
    let ends = edge
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            UnitOwnerTransportEvent::SharedCaptureEnd {
                environment: actual,
                source: symbol,
                owner: actual_owner,
                point,
            } => {
                assert_eq!(
                    (actual, symbol, actual_owner),
                    (*environment, source, owner)
                );
                Some((index, *point))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(end, point)] = ends.as_slice() else {
        panic!("one real branch capture end")
    };
    let drop_index = |expected: &UnitOwnerId| {
        let drops = edge
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == expected)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let [index] = drops.as_slice() else {
            panic!("one real owner retirement")
        };
        *index
    };
    let environment_drop = drop_index(environment);
    let source_drop = drop_index(owner);
    assert_eq!(environment_drop, end + 1);
    assert!(
        matches!(&edge[environment_drop], UnitOwnerTransportEvent::Drop { point: at, .. } if at == point)
    );
    assert!(source_drop > environment_drop);
    assert!(matches!(
        edge.last(),
        Some(UnitOwnerTransportEvent::UnitReturn { .. })
    ));
    (*end, environment_drop, source_drop)
}

// Ordinary source-only precondition for all three CI-only damaged-product consumers.
#[test]
fn terminal_branch_retirement_fixture_keeps_cleanup_inside_each_arm() {
    let fixture = Fixture::new(&[("root.ko", BRANCH_RETIREMENT)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    assert!(
        !observation.events().iter().any(|event| matches!(
            event,
            UnitOwnerTransportEvent::SharedCaptureEnd { .. } | UnitOwnerTransportEvent::Drop { .. }
        )),
        "both owners remain live before the fork"
    );
    let UnitOwnerTransportEvent::If {
        then_events,
        else_events,
        ..
    } = observation.events().last().unwrap()
    else {
        panic!("terminal If")
    };
    let returns = fixture.parsed[0]
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(
                node.payload(),
                crate::parser::Expression::Return { value: None, .. }
            )
            .then_some(UnitExpressionId::new(fixture.source(0), id))
        })
        .collect::<Vec<_>>();
    assert_eq!(returns.len(), 2);
    let text = fixture.symbol(0, "text");
    let source_owner = literal_owner(&fixture, 0, observation, "\"x\"");
    for ((edge, alias), exit) in [then_events, else_events]
        .into_iter()
        .zip(["left", "right"])
        .zip(returns)
    {
        let (end, environment_drop, source_drop) = retirement_indexes(observation, edge);
        let symbol = fixture.symbol(0, alias);
        let point = super::capture_composition::local_drop_point(&fixture, symbol);
        assert!(
            matches!(&edge[end], UnitOwnerTransportEvent::SharedCaptureEnd { source, owner, point: at, .. }
            if *source == text && *owner == source_owner && *at == point)
        );
        assert!(
            matches!(&edge[environment_drop], UnitOwnerTransportEvent::Drop { symbol: actual, point: at, .. }
            if *actual == symbol && *at == point)
        );
        assert!(
            matches!(&edge[source_drop], UnitOwnerTransportEvent::Drop { symbol, owner, .. }
            if *symbol == text && *owner == source_owner)
        );
        assert_eq!(
            edge.last(),
            Some(&UnitOwnerTransportEvent::UnitReturn { expression: exit })
        );
    }
}

#[test]
fn unused_terminal_capture_retires_before_the_fork() {
    let fixture = Fixture::new(&[("root.ko", SOURCES[0].0)]);
    let owned = fixture.shared();
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let point =
        super::capture_composition::local_drop_point(&fixture, fixture.symbol(0, "callback"));
    let ends = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::SharedCaptureEnd { point, .. } => Some(*point),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ends,
        [point],
        "the original unused capture ends at its declaration"
    );
    let UnitOwnerTransportEvent::If {
        then_events,
        else_events,
        ..
    } = observation.events().last().unwrap()
    else {
        panic!("terminal If")
    };
    for edge in [then_events, else_events] {
        assert!(matches!(
            edge.as_slice(),
            [UnitOwnerTransportEvent::UnitReturn { .. }]
        ));
    }
}
