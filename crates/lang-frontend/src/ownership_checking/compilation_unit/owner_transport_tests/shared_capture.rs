//! Real shared slots and unchanged transport across If, not conditional formation or loops.
use super::*;

pub(in crate::ownership_checking::compilation_unit) const SOURCES: &[&str] = &[
    r#"fun run(): Unit {
val text = "source"
val callback = { text == "test" }
val forwarded = callback
}"#,
    r#"fun run(): Unit {
val text = "source"
val first = { text == "one" }
val second = { text == "two" }
val moved_first = first
val moved_second = second
}"#,
    r#"fun run(): Unit {
var text = "old"
val previous = replace(&text, "new")
val callback = { text == previous }
val forwarded = callback
}"#,
    r#"fun run(): Unit {
val text = "source"
val callback = { text == "test" }
val forwarded = callback
val later = text
}"#,
];

#[test]
fn shared_string_environment_has_real_source_owner_slots() {
    for text in SOURCES {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observation = &owned.owner_observations()[0];
        assert_eq!(
            observation.status(),
            UnitOwnerObservationStatus::Complete,
            "{text}"
        );
        assert!(observation.is_consistent());
        let source = literal_owner(
            &fixture,
            0,
            observation,
            if text.contains("replace") {
                "\"new\""
            } else {
                "\"source\""
            },
        );
        let formations = observation
            .events()
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                UnitOwnerTransportEvent::SharedCapture {
                    expression,
                    environment,
                    inputs,
                } => Some((index, *expression, environment, inputs)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            formations.len(),
            if text.contains("moved_first") { 2 } else { 1 }
        );
        for (formed_at, expression, environment, inputs) in &formations {
            assert_eq!(
                observation
                    .definition((*environment).clone())
                    .unwrap()
                    .origin(),
                UnitOwnerOrigin::Closure(*expression)
            );
            assert_eq!(inputs[0], (fixture.symbol(0, "text"), source.clone()));
            if text.contains("replace") {
                assert_eq!(inputs.len(), 2);
                assert_eq!(
                    inputs[1],
                    (
                        fixture.symbol(0, "previous"),
                        literal_owner(&fixture, 0, observation, "\"old\"")
                    )
                );
            } else {
                assert_eq!(inputs.len(), 1);
            }
            let ends = observation
                .events()
                .iter()
                .enumerate()
                .filter_map(|(index, event)| match event {
                    UnitOwnerTransportEvent::SharedCaptureEnd {
                        point,
                        environment: actual,
                        source,
                        owner,
                    } if actual == *environment => Some((index, *point, *source, owner.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(ends.len(), inputs.len());
            assert_eq!(
                ends.iter()
                    .map(|(_, _, symbol, owner)| (*symbol, owner.clone()))
                    .collect::<Vec<_>>(),
                inputs.iter().rev().cloned().collect::<Vec<_>>()
            );
            let end_point = ends[0].1;
            assert!(
                ends.iter()
                    .all(|(index, point, _, _)| index > formed_at && *point == end_point)
            );
            assert!(ends.windows(2).all(|pair| pair[1].0 == pair[0].0 + 1));
            let env_drop = ends.last().unwrap().0 + 1;
            assert!(
                matches!(&observation.events()[env_drop], UnitOwnerTransportEvent::Drop { point, owner, .. } if *point == end_point && owner == *environment)
            );
            for (_, borrowed) in *inputs {
                let source_drop = observation.events().iter().position(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == borrowed)).unwrap();
                assert!(
                    source_drop > env_drop,
                    "source cannot die before its last environment"
                );
            }
        }
        // A shared slot never steals its source into an owned capture.
        assert!(!observation.events().iter().any(|event| matches!(
            event,
            UnitOwnerTransportEvent::Capture { .. } | UnitOwnerTransportEvent::CaptureDrop { .. }
        )));
        assert!(observation.value_observation_v1().is_none());
    }
}

pub(in crate::ownership_checking::compilation_unit) const BOUNDARIES: &[&str] = &[
    r#"fun run(text: String): Unit { val callback = { text == "x" } }"#,
    r#"fun run(): Unit { val text = "x"; val inner = { text == "x" }; val outer = { inner() } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; while (flag) { val callback = { text == "x" } } }"#,
    r#"fun inspect(callback: () -> Boolean): Unit {}
fun run(): Unit { val text = "x"; inspect({ text == "x" }) }"#,
    r#"fun run(number: Int): Unit { val text = "x"; val callback = { text == "x" && number == 0 } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { return } else {}; val forwarded = callback }"#,
];

pub(in crate::ownership_checking::compilation_unit) const CROSS_IF_SOURCES: &[&str] = &[
    r#"fun run(flag: Boolean): Unit {
val text = "source"
val callback = { text == "test" }
if (flag) { val unrelated = "left" } else { val unrelated = "right" }
val forwarded = callback
}"#,
    r#"fun run(flag: Boolean): Unit {
val text = "source"
val callback = { text == "test" }
if (flag) { if (flag) {} else {} } else { if (flag) {} else {} }
val forwarded = callback
}"#,
    r#"fun run(flag: Boolean): Unit {
val text = "source"
val callback = { text == "test" }
val second = { text == "other" }
if (flag) {} else {}
val forwarded = callback
if (flag) {} else {}
val forwarded_second = second
}"#,
    r#"fun run(flag: Boolean): Unit {
val text = "source"
val callback = { text == "test" }
var unrelated = "other"
if (flag) { var local = "left"; val old = replace(&local, "next") } else { var local = "right"; swap(&local, &unrelated); swap(&local, &unrelated) }
val forwarded = callback
}"#,
];

#[test]
fn shared_capture_live_across_independent_if_keeps_source_slots() {
    for text in CROSS_IF_SOURCES {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observation = &owned.owner_observations()[0];
        assert_eq!(
            observation.status(),
            UnitOwnerObservationStatus::Complete,
            "{text}"
        );
        assert!(observation.is_consistent());
        let source = literal_owner(&fixture, 0, observation, "\"source\"");
        let events = observation.events();
        let mut formations = 0;
        for (index, event) in events.iter().enumerate() {
            let UnitOwnerTransportEvent::SharedCapture {
                environment,
                inputs,
                ..
            } = event
            else {
                continue;
            };
            formations += 1;
            assert_eq!(inputs, &[(fixture.symbol(0, "text"), source.clone())]);
            let end = events
                .iter()
                .position(|event| {
                    matches!(event,
                UnitOwnerTransportEvent::SharedCaptureEnd { environment: actual, owner, .. }
                if actual == environment && owner == &source)
                })
                .unwrap();
            assert!(
                events[index + 1..end]
                    .iter()
                    .any(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
            );
            assert!(
                matches!(&events[end + 1], UnitOwnerTransportEvent::Drop { owner, .. } if owner == environment)
            );
            let source_end = events.iter().position(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == &source)).unwrap();
            assert!(source_end > end + 1);
        }
        assert_eq!(formations, if text.contains("val second") { 2 } else { 1 });
        assert!(observation.value_observation_v1().is_none());
    }
}

#[test]
fn shared_capture_outside_the_named_straight_line_slice_stays_unsupported() {
    for text in BOUNDARIES {
        let owned = checked(text);
        let observation = owned
            .owner_observations()
            .iter()
            .rev()
            .find(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
            .unwrap();
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

// Corrupted observation products are CI-only validation tests, not local runtime probes.
#[test]
fn shared_capture_validator_rejects_missing_duplicate_reordered_and_foreign_ends() {
    let fixture = Fixture::new(&[("root.ko", SOURCES[2])]);
    let owned = fixture.shared();
    let original = &owned.owner_observations()[0];
    let start = original
        .events
        .iter()
        .position(|event| matches!(event, UnitOwnerTransportEvent::SharedCapture { .. }))
        .unwrap();
    let end = original
        .events
        .iter()
        .position(|event| matches!(event, UnitOwnerTransportEvent::SharedCaptureEnd { .. }))
        .unwrap();
    let mut missing = original.clone();
    missing.events.remove(end);
    assert!(!missing.is_consistent());
    let mut duplicate = original.clone();
    duplicate.events.insert(end, original.events[end].clone());
    assert!(!duplicate.is_consistent());
    let mut reordered = original.clone();
    reordered.events.swap(end, end + 1);
    assert!(!reordered.is_consistent());
    let mut early_source_drop = original.clone();
    let source_drop = early_source_drop.events.remove(end + 3);
    early_source_drop.events.insert(end, source_drop);
    assert!(!early_source_drop.is_consistent());
    for empty in [false, true] {
        let mut damaged = original.clone();
        let UnitOwnerTransportEvent::SharedCapture { inputs, .. } = &mut damaged.events[start]
        else {
            panic!("formation")
        };
        if empty {
            inputs.clear();
        } else {
            inputs.push(inputs[0].clone());
        }
        assert!(!damaged.is_consistent());
    }
    let mut wrong_point = original.clone();
    let UnitOwnerTransportEvent::SharedCaptureEnd { point, .. } = &mut wrong_point.events[end]
    else {
        panic!("end")
    };
    *point = UnitDropPoint::AfterExpression(UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "callback"),
    ));
    assert!(!wrong_point.is_consistent());
    let foreign = fixture.shared();
    let UnitOwnerTransportEvent::SharedCapture {
        environment,
        inputs,
        ..
    } = &foreign.owner_observations()[0].events[start]
    else {
        panic!("formation")
    };
    for replace_environment in [false, true] {
        let mut damaged = original.clone();
        let UnitOwnerTransportEvent::SharedCaptureEnd {
            environment: actual,
            owner,
            ..
        } = &mut damaged.events[end]
        else {
            panic!("end")
        };
        if replace_environment {
            *actual = environment.clone();
        } else {
            *owner = inputs[1].1.clone();
        }
        assert!(!damaged.is_consistent());
        assert_ne!(original, &damaged);
    }
}

pub(in crate::ownership_checking::compilation_unit) const RETIRE_IF_SOURCES: &[&str] = &[
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { val forwarded = callback } else { val forwarded = callback } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { if (flag) { val a = callback } else { val b = callback } } else { if (flag) { val c = callback } else { val d = callback } } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; val second = { text == "y" }; if (flag) { val a = callback } else { val b = callback }; val remaining = second }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) { val a = callback } else { val b = callback }; val later = text }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val other = "y"; val callback = { text == other }; if (flag) { val a = callback } else { val b = callback } }"#,
];

// Enumerate real alternatives for independent per-path order assertions.
pub(super) fn paths(events: &[UnitOwnerTransportEvent]) -> Vec<Vec<&UnitOwnerTransportEvent>> {
    let mut result = vec![Vec::new()];
    for event in events {
        if let UnitOwnerTransportEvent::If {
            then_events,
            else_events,
            ..
        } = event
        {
            let alternatives = paths(then_events).into_iter().chain(paths(else_events));
            result = alternatives
                .flat_map(|edge| {
                    result.iter().map(move |prefix| {
                        let mut path = prefix.clone();
                        path.extend(&edge);
                        path
                    })
                })
                .collect();
        } else {
            for path in &mut result {
                path.push(event);
            }
        }
    }
    result
}

#[test]
fn shared_capture_retires_on_both_if_edges_without_phi() {
    for text in RETIRE_IF_SOURCES {
        let owned = checked(text);
        let observation = &owned.owner_observations()[0];
        assert_eq!(
            observation.status(),
            UnitOwnerObservationStatus::Complete,
            "{text}"
        );
        assert!(observation.is_consistent());
        assert!(observation.value_observation_v1().is_none());
        assert!(
            observation
                .events()
                .iter()
                .any(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
        );
        let (first_environment, slot_count) = observation
            .events()
            .iter()
            .find_map(|event| match event {
                UnitOwnerTransportEvent::SharedCapture {
                    environment,
                    inputs,
                    ..
                } => Some((environment, inputs.len())),
                _ => None,
            })
            .unwrap();
        let (branch_at, then_events, else_events) = observation
            .events()
            .iter()
            .enumerate()
            .find_map(|(index, event)| match event {
                UnitOwnerTransportEvent::If {
                    then_events,
                    else_events,
                    ..
                } => Some((index, then_events, else_events)),
                _ => None,
            })
            .unwrap();
        for edge in [then_events, else_events] {
            for leaf in paths(edge) {
                assert_eq!(leaf.iter().filter(|event| matches!(event, UnitOwnerTransportEvent::SharedCaptureEnd { environment, .. } if environment == first_environment)).count(), slot_count);
                assert_eq!(leaf.iter().filter(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == first_environment)).count(), 1);
            }
        }
        assert!(!observation.events()[branch_at + 1..].iter().any(|event| matches!(event,
            UnitOwnerTransportEvent::SharedCaptureEnd { environment, .. } if environment == first_environment)));
        let alternatives = paths(observation.events());
        assert_eq!(
            alternatives.len(),
            if text.contains("val c =") { 4 } else { 2 }
        );
        for path in alternatives {
            let mut formations = 0;
            for (at, event) in path.iter().enumerate() {
                let UnitOwnerTransportEvent::SharedCapture {
                    environment,
                    inputs,
                    ..
                } = event
                else {
                    continue;
                };
                formations += 1;
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
                for ((index, source, owner, point), (expected_source, expected_owner)) in
                    ends.iter().zip(inputs.iter().rev())
                {
                    assert!(*index > at);
                    assert_eq!(source, expected_source);
                    assert_eq!(*owner, expected_owner);
                    assert_eq!(*point, ends[0].3);
                    assert!(path[*index + 1..].iter().any(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner: dropped, .. } if dropped == *owner)));
                }
                assert!(ends.windows(2).all(|pair| pair[0].0 + 1 == pair[1].0));
                let last = ends.last().unwrap();
                assert!(
                    matches!(path[last.0 + 1], UnitOwnerTransportEvent::Drop { owner, point, .. } if owner == environment && point == last.3)
                );
            }
            assert_eq!(formations, if text.contains("val second") { 2 } else { 1 });
        }
    }
}

// CI-only malformed event controls, compiled but not executed by local acceptance.
#[test]
fn branch_retirement_authenticates_edge_and_keeps_other_loans_live() {
    for text in [RETIRE_IF_SOURCES[0], RETIRE_IF_SOURCES[2]] {
        let owned = checked(text);
        let original = &owned.owner_observations()[0];
        assert!(original.is_consistent());
        for in_then in [true, false] {
            let mut damaged = original.clone();
            let UnitOwnerTransportEvent::If {
                control,
                then_events,
                else_events,
            } = damaged
                .events
                .iter_mut()
                .find(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
                .unwrap()
            else {
                unreachable!()
            };
            let edge = if in_then { then_events } else { else_events };
            let wrong_point = UnitDropPoint::BranchExit {
                control: *control,
                branch: if in_then { 1 } else { 0 },
            };
            // Changing both End and Drop avoids relying on their adjacency check.
            for event in edge {
                match event {
                    UnitOwnerTransportEvent::SharedCaptureEnd { point, .. }
                    | UnitOwnerTransportEvent::Drop { point, .. } => *point = wrong_point,
                    _ => {}
                }
            }
            assert!(!damaged.is_consistent());
        }
    }
    let owned = checked(RETIRE_IF_SOURCES[2]);
    let original = &owned.owner_observations()[0];
    for in_then in [true, false] {
        let mut damaged = original.clone();
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
        let edge = if in_then { then_events } else { else_events };
        let (at, source, owner, point) = edge
            .iter()
            .enumerate()
            .find_map(|(at, event)| match event {
                UnitOwnerTransportEvent::SharedCaptureEnd {
                    source,
                    owner,
                    point,
                    ..
                } => Some((at, *source, owner.clone(), *point)),
                _ => None,
            })
            .unwrap();
        edge.insert(
            at + 2,
            UnitOwnerTransportEvent::Drop {
                point,
                symbol: source,
                owner,
            },
        );
        assert!(!damaged.is_consistent());
    }
}

// Exact source previously classified as unsupported by the Owned-only boundary test.
pub(in crate::ownership_checking::compilation_unit) const REPLACED_SHARED_SOURCE: &str =
    "fun run(): Unit { var a = \"old\"; val old = replace(&a, \"new\"); val callback = { a } }";

#[test]
fn shared_replaced_source_keeps_real_owner_and_ordered_retirement() {
    let fixture = Fixture::new(&[("root.ko", REPLACED_SHARED_SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let old = literal_owner(&fixture, 0, observation, "\"old\"");
    let current = literal_owner(&fixture, 0, observation, "\"new\"");
    let events = observation.events();
    let captures = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            UnitOwnerTransportEvent::SharedCapture {
                expression,
                environment,
                inputs,
            } => Some((index, expression, environment, inputs)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, expression, environment, inputs)] = captures.as_slice() else {
        panic!("one Shared environment")
    };
    assert_eq!(
        **expression,
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{ a }"))
    );
    assert_eq!(
        observation
            .definition((*environment).clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::Closure(**expression)
    );
    assert_eq!(
        inputs.as_slice(),
        &[(fixture.symbol(0, "a"), current.clone())]
    );
    assert_ne!(**environment, current);
    assert_ne!(current, old);
    let ends = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            UnitOwnerTransportEvent::SharedCaptureEnd {
                environment: actual,
                source,
                owner,
                point,
            } if actual == *environment => Some((index, *source, owner, point)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(end, source, owner, point)] = ends.as_slice() else {
        panic!("one exact loan end")
    };
    assert!(end > at);
    assert_eq!(*source, fixture.symbol(0, "a"));
    assert_eq!(*owner, &current);
    assert!(
        matches!(&events[end + 1], UnitOwnerTransportEvent::Drop { owner, point: at, .. } if owner == *environment && at == *point)
    );
    assert!(events[end + 2..].iter().any(|event| matches!(event, UnitOwnerTransportEvent::Drop { symbol, owner, .. } if *symbol == fixture.symbol(0, "a") && owner == &current)));
    assert!(!events.iter().any(|event| matches!(
        event,
        UnitOwnerTransportEvent::Capture { .. } | UnitOwnerTransportEvent::CaptureDrop { .. }
    )));
    assert!(!events[at + 1..*end].iter().any(
        |event| matches!(event, UnitOwnerTransportEvent::Move { owner, .. } if owner == &current)
    ));
    assert!(observation.value_observation_v1().is_none());
}
