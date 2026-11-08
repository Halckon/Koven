//! A real terminal edge must not contribute retired identities to its continuation.
use super::*;

pub(in crate::ownership_checking::compilation_unit) const SOURCES: &[&str] = &[
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { return } else {}; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) {} else { return }; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; val second = { text == "y" }; if (flag) { return } else {}; val forwarded = callback; val other = second }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val other = "y"; val callback = { text == other }; if (flag) { return } else {}; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { if (flag) { return } else {} } else {}; val forwarded = callback }"#,
];

pub(in crate::ownership_checking::compilation_unit) const BOUNDARIES: &[&str] = &[
    r#"fun run(flag: Boolean): String { val text = "x"; if (flag) { return text } else {}; return "y" }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { return } else { return } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { val local = "local"; return } else {}; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { return }; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; while (flag) { return }; val forwarded = callback }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { return } else {}; callback() }"#,
];

// Unlike the ordinary paths helper, stop at a real UnitReturn, never append suffix events.
fn paths(events: &[UnitOwnerTransportEvent]) -> Vec<Vec<&UnitOwnerTransportEvent>> {
    let mut result: Vec<Vec<&UnitOwnerTransportEvent>> = vec![Vec::new()];
    for event in events {
        let mut next = Vec::new();
        for prefix in result {
            if matches!(
                prefix.last(),
                Some(UnitOwnerTransportEvent::UnitReturn { .. })
            ) {
                next.push(prefix);
            } else if let UnitOwnerTransportEvent::If {
                then_events,
                else_events,
                ..
            } = event
            {
                for arm in paths(then_events).into_iter().chain(paths(else_events)) {
                    let mut path = prefix.clone();
                    path.extend(arm);
                    next.push(path);
                }
            } else {
                let mut path = prefix;
                path.push(event);
                next.push(path);
            }
        }
        result = next;
    }
    result
}

#[test]
fn shared_capture_unit_return_edge_does_not_retire_the_continuation() {
    for text in SOURCES {
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
        let alternatives = paths(observation.events());
        assert_eq!(
            alternatives
                .iter()
                .filter(|path| matches!(
                    path.last(),
                    Some(UnitOwnerTransportEvent::UnitReturn { .. })
                ))
                .count(),
            1
        );
        assert!(alternatives.iter().any(|path| !matches!(
            path.last(),
            Some(UnitOwnerTransportEvent::UnitReturn { .. })
        )));
        for path in alternatives {
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
                    .filter_map(|(at, event)| match event {
                        UnitOwnerTransportEvent::SharedCaptureEnd {
                            environment: actual,
                            source,
                            owner,
                            point,
                        } if actual == environment => Some((at, *source, owner, point)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(ends.len(), inputs.len());
                for (end, input) in ends.iter().zip(inputs.iter().rev()) {
                    assert_eq!((end.1, end.2), (input.0, &input.1));
                }
                assert!(ends.windows(2).all(|pair| pair[0].0 + 1 == pair[1].0));
                let end = ends.last().unwrap();
                assert!(
                    matches!(path[end.0 + 1], UnitOwnerTransportEvent::Drop { owner, point, .. } if owner == environment && point == end.3)
                );
                for (_, source) in inputs {
                    let source_drop = path.iter().position(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == source)).unwrap();
                    assert!(source_drop > end.0 + 1);
                }
            }
            if let Some(UnitOwnerTransportEvent::UnitReturn { expression }) = path.last() {
                let expected =
                    UnitExpressionId::new(fixture.source(0), fixture.expression(0, "return"));
                assert_eq!(*expression, expected);
                for event in &path {
                    match event {
                        UnitOwnerTransportEvent::SharedCaptureEnd { point, .. }
                        | UnitOwnerTransportEvent::Drop { point, .. } => {
                            assert_eq!(*point, UnitDropPoint::ControlTransfer(expected))
                        }
                        _ => {}
                    }
                }
                assert!(
                    !path
                        .iter()
                        .any(|event| matches!(event, UnitOwnerTransportEvent::Move { .. }))
                );
            } else {
                assert!(
                    path.iter()
                        .any(|event| matches!(event, UnitOwnerTransportEvent::Move { .. }))
                );
            }
        }
    }
}

#[test]
fn unit_return_keeps_unsupported_terminal_shapes_fail_closed() {
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

#[test]
fn original_plain_return_boundary_keeps_the_surviving_owner_state() {
    // Exact original branch-boundary input, now checked positively, not discarded.
    let source =
        super::branch_boundaries::source(r#"if (flag) { return } else { val local = "right" }"#);
    let owned = checked(&source);
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let alternatives = paths(observation.events());
    assert_eq!(alternatives.len(), 2);
    assert_eq!(
        alternatives
            .iter()
            .filter(|path| matches!(
                path.last(),
                Some(UnitOwnerTransportEvent::UnitReturn { .. })
            ))
            .count(),
        1
    );
}

// CI-only malformed product tests; not executed by local acceptance.
#[test]
fn unit_return_rejects_incomplete_cleanup_missing_exit_and_suffix_events() {
    let owned = checked(SOURCES[0]);
    let original = &owned.owner_observations()[0];
    for damage in 0..7 {
        let mut damaged = original.clone();
        let UnitOwnerTransportEvent::If {
            then_events: edge, ..
        } = damaged
            .events
            .iter_mut()
            .find(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        let exit = edge.last().unwrap().clone();
        assert!(matches!(exit, UnitOwnerTransportEvent::UnitReturn { .. }));
        let end = edge
            .iter()
            .position(|event| matches!(event, UnitOwnerTransportEvent::SharedCaptureEnd { .. }))
            .unwrap();
        match damage {
            0 => {
                edge.remove(end);
            }
            1 => {
                edge.remove(end + 1);
            }
            2 => {
                edge.remove(end + 2);
            }
            3 => {
                edge.pop();
            }
            4 => {
                edge.push(exit);
            }
            5 => {
                edge.insert(end, exit);
            }
            _ => {
                // Pretending the continuing branch terminates cannot drop live slots.
                let UnitOwnerTransportEvent::If { else_events, .. } = damaged
                    .events
                    .iter_mut()
                    .find(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                else_events.push(exit);
            }
        }
        assert!(!damaged.is_consistent());
    }
}

#[test]
fn unit_return_identity_must_belong_to_the_same_source() {
    let fixture = Fixture::new(&[
        ("root.ko", SOURCES[0]),
        ("other.ko", "fun other(): Unit { return }"),
    ]);
    let owned = fixture.shared();
    let mut damaged = owned.owner_observations().iter().find(|observation| matches!(
        observation.callable(), UnitOwnerCallable::Function(item) if item.source_unit() == fixture.source(0)
    )).unwrap().clone();
    assert!(damaged.is_consistent());
    assert_ne!(fixture.source(0), fixture.source(1));
    let UnitOwnerTransportEvent::If { then_events, .. } = damaged
        .events
        .iter_mut()
        .find(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    let UnitOwnerTransportEvent::UnitReturn { expression } = then_events.last_mut().unwrap() else {
        unreachable!()
    };
    assert_eq!(expression.source_unit(), fixture.source(0));
    *expression = UnitExpressionId::new(fixture.source(1), fixture.expression(1, "return"));
    assert!(!damaged.is_consistent());
}

#[test]
fn unit_return_cleanup_does_not_authorize_continuing_source_consumption() {
    let owned = checked(SOURCES[0]);
    let mut damaged = owned.owner_observations()[0].clone();
    assert!(damaged.is_consistent());
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
    let (source, owner, point) = then_events
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::SharedCaptureEnd {
                source,
                owner,
                point,
                ..
            } => Some((*source, owner.clone(), *point)),
            _ => None,
        })
        .unwrap();
    else_events.push(UnitOwnerTransportEvent::Drop {
        symbol: source,
        owner,
        point,
    });
    assert!(!damaged.is_consistent());
}
